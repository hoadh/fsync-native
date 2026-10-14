# Review: infra/smtp-sg-relay (602084e, 0c8cb41, e0c3b26) vs 2b96829

Score: 8.5/10. No critical issues. Two high, five medium, a few low.
Verified: `cargo test -p fsync-backend` passes (23 unit tests, 7 mailpit tests, all other suites). shellcheck at `-S style` shows only pre-existing info notes. The dependency tree has ring and webpki-roots only (no openssl, native-tls or aws-lc). No `.env` file is tracked in git.

## Critical
None.

## High
H1. deploy/scripts/deploy.sh (the `compose up -d --no-build --remove-orphans` line, ~137): `--relay-only` is not guarded against a host that already runs the full stack.
- Both compose files use project name `fsync` and the same dir `/opt/fsync`.
- Running `--relay-only` against cms-node (or the main host) swaps compose.yml for the relay-only file.
- `--remove-orphans` then deletes the backend and postgres containers. The volume survives, but the service goes down.
- The reverse (a rollback without `--relay-only`) is safe: it dies early in `have_image`.
- Fix, either of:
  - In relay-only mode, die if the host's current compose.yml defines `backend:` or `postgres:`.
  - Drop `--remove-orphans` when `relay_only=1`.

H2. deploy/.env.example:24-25 (copied by gen-env.sh): a fresh production `.env` ships `EMAIL_SENDER=log` and `ALLOW_LOG_EMAIL=1`.
- A new production host starts cleanly but cannot deliver OTP codes.
- In production, `dev_mode=0`, so the code is not logged at any level. Nobody can sign in, and only one startup `warn` says so.
- The refuse-to-start guard is defeated by the default value.
- The deploy.sh legacy append is justified. The new-host default is not.
- Fix: gen-env should leave `ALLOW_LOG_EMAIL` commented out or empty, with an explicit opt-in. Keep the deploy.sh legacy append.

## Medium
M1. backend/src/auth/otp.rs:70: `limiter.record(&email_key)` runs before `send_otp`.
- A failed send burns one of the 5 per-email-per-hour slots, and the per-IP slot too.
- The OTP row is deleted on failure (lines 72-80), so nothing live is left. That is correct.
- During a Gmail outage each user is locked out for an hour after 5 retries, even after SMTP recovers.
- Acceptable pre-launch.
- Better: refund the email slot on failure. Document it either way.

M2. otp.rs `start`: concurrent `start` calls for the same email during the send (up to 30 s).
- A second caller sees `created == 0` and returns 204 for a code that is still in flight.
- If the first send fails, the row is deleted and the second caller was told "sent".
- Low probability, and the user can retry. Note it.

M3. There is no global send cap.
- Gmail allows about 500 recipients a day. 20 per IP per hour, distributed across IPs, can exhaust that and block OTP for everyone.
- It is also an arbitrary-recipient mail bomb, limited to 5 per address per hour.
- Add a global daily budget or a circuit breaker. Record it as a launch blocker (the README already says to use a transactional provider before launch).

M4. deploy.sh legacy append (~lines 124-130): the grep is `^EMAIL_SENDER=smtp`.
- A quoted value (`EMAIL_SENDER="smtp"`) is not matched, so the host gets a redundant `ALLOW_LOG_EMAIL=1`. This is benign but invisible later.
- The append is atomic (`.env.new` with umask 077, then `mv`). It is idempotent, because a later run matches `^ALLOW_LOG_EMAIL=`. It runs under the flock.
- The cms-node staging case works. `FSYNC_ENV=staging` is not production, and a missing FSYNC_ENV defaults to production in compose, which the append covers.
- Confirm the real cms-node `.env` once after deploy. The stderr warning is the only signal.

M5. Test gaps:
- The 30 s `SEND_TIMEOUT` and the 10 s IO timeout are not exercised. The dead-port test fails fast, with connection refused.
- There is no CRLF recipient test.
  - It is safe in practice. `normalize_email` rejects whitespace and control characters before the sender, and lettre's `Mailbox` parser rejects them again.
  - A one-line regression test is cheap.

## Low
L1. smtp.rs:50-56: `cfg.password.to_string()` makes an un-zeroized copy inside lettre `Credentials`. Unavoidable.

L2. compose.yml:63-70: the SMTP password is a container env var, so `docker inspect` shows it to root or docker-group users.
- This is consistent with POSTGRES_PASSWORD and the JWT key.
- `.env` is 600 via a `umask 077` temp file plus `mv`. deploy.sh never echoes it. There is no `set -x`. The failure path prints only `compose logs`, which have category-level SMTP errors.
- gen-env.sh and the CI `config -q` run only with an empty password.

L3. Rollback in relay-only mode requires repeating `--relay-only`. It is non-destructive without the flag (the early image check dies), but the error text is misleading. Auto-detect it from compose.yml, or improve the die message.

L4. deploy.sh copies the unused Caddyfile to the relay host. Harmless. Relay-only never touches Caddy and never stops other containers: only `docker compose` in project `fsync` is used.

L5. compose.relay-only.yml binds `127.0.0.1:7842/udp` even with QAD off, and creates an unused `relay_nocert` volume. Harmless.

L6. ci.yml:57: the mailpit pre-pull is good, but the tag v1.31.5 is duplicated in tests/common/mailpit.rs. Add a cross-reference comment. deploy-stack.yml `config -q` for the relay-only file is correct.

## Verified OK
- TLS:
  - `Tls::Required` for 587 and `Tls::Wrapper` for 465, with no plaintext fallback.
  - `builder_dangerous` is only the lettre constructor name. The TLS mode is overridden afterwards, and no accept-invalid-certs setting is used.
  - Tests cover plaintext refusal and an untrusted certificate.
- Secrets:
  - The password is a `Zeroizing` value and is redacted in `Debug` for `SmtpConfig` and `Config`.
  - Errors carry only a category and a status code, and tests assert the password, user, address and code are absent.
  - The API returns a generic 500 with no leak.
- Production never logs the code. `LogSender` logs a blake3 tag only, and the debug code line needs dev mode, which production refuses.
- deploy.sh:
  - A flock on `.deploy.lock` and a unique `.incoming.XXXXXX` staging dir.
  - Image pruning is limited to fsync images, and relay-only prunes only the relay image.
  - Smoke checks go over loopback.
- File sizes are all under 200 lines (config.rs 189, tests/common/mod.rs 188, smtp.rs 125).

## Unresolved questions
- What is the cms-node staging `.env` FSYNC_ENV (`staging` or unset)? That decides whether the legacy append actually mattered.
- Is the Gmail-outage lockout (M1) acceptable, or should the quota be refunded on failure?
- Is Gmail only for pre-launch? If so, add a ticket for a global send budget.
