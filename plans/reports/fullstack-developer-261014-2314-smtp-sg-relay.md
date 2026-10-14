# S1 SMTP OTP + S2 relay-only: implementation report

Branch `infra/smtp-sg-relay` (from main 2b96829), worktree `/Users/alex/Code/fsync-native/.claude/worktrees/agent-a64180850ad3a82c7`. Not pushed.
Commits: 602084e (backend SMTP), 0c8cb41 (deploy SMTP env + docs), e0c3b26 (relay-only).

## S1
- `backend/src/email/{config,smtp}.rs`: SmtpConfig/EmailMode validation, SmtpSender (lettre 0.11, rustls/ring only, TLS mandatory: STARTTLS 587, implicit 465; IO timeout 10 s, send 30 s). Errors reduced to category + status code; password zeroized and redacted in Debug; OTP never logged. API returns generic 500 and frees the unsent code.
- Startup: production + non-smtp sender fails unless `ALLOW_LOG_EMAIL` truthy. gen-env (via .env.example) writes `ALLOW_LOG_EMAIL=1`; deploy.sh appends it (warning) to a legacy remote .env lacking both settings, so staging keeps working.
- Tests: 7 integration tests against real pinned mailpit v1.31.5 container (STARTTLS+auth, per-run rcgen cert) + 4 unit tests of the config matrix. CI pulls the image.
- Docs: docs/api.md config-env; deploy/README.md "OTP email (Gmail SMTP)" (App Password steps, ~500/day).

## S2 (redesigned to shared host per coordinator)
- `deploy/compose.relay-only.yml` (relay only, loopback 127.0.0.1:18081, no ACME, QAD off by default, 128 MB), `deploy/relay-only.env.example`, `deploy.sh --relay-only` (docker-missing error points to README), `smoke-check.sh --relay-only`, `scripts/test-relay-only-local.sh`.
- README "Relay-only host": Docker install (docker-ce apt repo, Ubuntu 24.04), deploy, Caddy snippet for the owner (nothing edits Caddy), DNS grey cloud, firewall.
- Firewall: nothing new by default (22/80/443 open). udp/7842 only if QAD enabled; QAD off for parity with the main host.
- CI: deploy-stack.yml also runs `config -q` on the relay-only compose.

## Gates
fmt check ok; clippy -D warnings ok; `cargo test --workspace` all pass (incl. mailpit); shellcheck ok; `docker compose config -q` ok for both files; local relay-only run (real Docker build, healthy, smoke ok, only relay, ports loopback only), cleaned up.

## Not verified
Real Gmail delivery, real host Caddy + relay on the SG host, QAD on the new host. deploy.sh still ships an unused Caddyfile in relay-only mode (harmless). Rollback in relay-only needs `--relay-only` again.

## Unresolved questions
- Enable QAD on the SG relay? (needs udp/7842 rule and cert dir mount.)
- Gmail sender address for SMTP_FROM.
- Report is in the worktree, not the requested shared path (tool refused the shared path from an isolated worktree).

## Review fixes (code-reviewer-261015-0017)
- H1: deploy.sh host-mode guard before any file swap or `up`. Mode = `$dir/.mode` marker (`full`|`relay-only`, written every non-rollback deploy); hosts without it are inferred from compose.yml (`backend:`/`postgres:`) or running fsync-project backend/postgres containers. `--relay-only` on a full host dies with instructions; full deploy on a relay-only host dies ("pass --relay-only"); `--rollback` on a relay-only host auto-selects relay-only. Checked with an ssh shim against a fake host dir (both dies, compose.yml untouched).
- H2: .env.example (so gen-env) now `EMAIL_SENDER=smtp`, empty SMTP_USERNAME/PASSWORD/SMTP_FROM, no ALLOW_LOG_EMAIL: production refuses to start until filled. Legacy append kept. M4: grep now matches quoted `EMAIL_SENDER="smtp"`.
- M3: global cap `OTP_DAILY_SEND_CAP` (default 450, rolling 24 h) in Postgres (`otp_send_log`, migration 0003, advisory-lock reservation so concurrent starts cannot overshoot; survives restarts and multi-instance). Cap hit: 429 + Retry-After, nothing sent, code row removed. Passed through compose.yml and .env.example. Applies to every sender (also log).
- M1: per-email slot recorded only after a successful send; per-IP slot and global slot refunded on failure or cap refusal (`RateLimiter::unrecord`, `otp_budget::release`). Per-IP refund pops the newest hit for that IP (not necessarily this request's); acceptable.
- M5: test `silent_server_ends_in_a_timeout_error` (TCP listener that accepts and never speaks). Finding: lettre's 10 s IO timeout does not cover the greeting read; the 30 s SEND_TIMEOUT is what fires, so the test takes ~30 s. CRLF recipient test not added (normalize_email + Mailbox parse already reject; not requested).
- Docs: deploy/README.md OTP section, docs/deployment-guide.md stale "no email sender" line.
- Gates: fmt, clippy -D warnings, `cargo test -p fsync-backend` all pass; shellcheck only the pre-existing SC2029 info; compose config -q OK for both files (with gen-env output / relay-only.env.example).
- Not done: M2 (concurrent start sees 204 while send in flight) remains a documented low-probability edge. cms-node: its .env has EMAIL_SENDER=smtp so the legacy append is skipped; first deploy after this writes `.mode=full` (host has compose.yml with backend, so inference is full).
