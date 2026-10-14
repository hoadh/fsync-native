# Round 2 review: infra/smtp-sg-relay (e0c3b26..bed3de2)
Score 9.3/10. Critical 0, High 0, Medium 2, Low 4.
Gates: fmt OK, clippy -D warnings OK, cargo test -p fsync-backend all pass (smtp_mailpit 30.0 s, the silent-server test), shellcheck info-level only (SC2016/SC2029, pre-existing style).

## Round-1 fixes
- H1 OK: guard runs before image build, staging dir, .env upload and the swap. Full+--relay-only dies. Relay-only host + plain deploy dies. Rollback on a relay-only host auto-selects relay-only (guard runs before the tag/images/services selection). .mode is written inside the flock section.
- H2/M4 OK: quoted EMAIL_SENDER regex is correct for smtp, "smtp", 'smtp' and trailing whitespace.
- M1 OK: failed send refunds global slot + per-IP, code row deleted, per-email counted on success only.
- M3 OK: advisory xact lock serializes reserve; prune+count+insert in one tx; cap refusal rolls back (no leak, expired rows re-pruned next call). Retry-After = oldest+24h-now, min 1 s, header verified by test. otp_send_log bounded by cap (+ rare failed releases, pruned within 24 h).
- M5 OK: real TcpListener that never greets, no mocks, asserts "timeout" and <40 s. 30 s runtime is acceptable but is now the longest test in the suite.

## cms-node next deploy
.mode absent, compose.yml has `^  backend:`/`^  postgres:` (services at 2-space indent, compose.yml:18,38) so inferred full; plain deploy proceeds. .env has EMAIL_SENDER=smtp so legacy_log=0, no ALLOW_LOG_EMAIL injected. OTP_DAILY_SEND_CAP absent in .env, compose default 450 applies. Migration 0003 is additive (new table + index, no touch of existing data), run by sqlx at backend start, down migration present. Safe. .mode gets written on that deploy.

## Medium
- M-A deploy/scripts/deploy.sh:55-58 (+ compose*.yml:8,9). Container inference hardcodes project label `fsync`, and both compose files name the project `${COMPOSE_PROJECT_NAME:-fsync}` regardless of dir. A fresh `--dir /opt/other --relay-only` on a host whose full stack is in /opt/fsync is blocked (no .mode, no compose.yml, but project=fsync containers exist), so the die message "use a separate --dir" at :65 is a dead end unless COMPOSE_PROJECT_NAME is also set in that dir's .env. Without the guard the same-name project would make --remove-orphans delete the full stack, so blocking is right but the advice is wrong. Fix: reword to "use a dedicated host, or a separate --dir with a distinct COMPOSE_PROJECT_NAME"; ideally derive the label from the .env value / basename of $dir.
- M-B backend/src/auth/otp.rs:71-77 (+ email timeout). Refund-on-error treats an ambiguous failure (client timeout after the server accepted DATA, 30 s overall timeout) as "not delivered": slot, per-IP and per-email all refunded and code deleted though Gmail may have delivered. An attacker who can induce timeouts could mail one victim beyond the per-email/global limits, and a user would receive a code that is already deleted. Low likelihood (needs control of provider latency) but it is the one path where the quota understates sends. Mitigation: do not refund the global slot (and per-email) on timeout-class errors, refund only on definite failures (connect/auth/4xx-5xx reply).

## Low
- L1 otp.rs:73 / test cap_refusal_does_not_burn_the_ip_quota: refunding per-IP on a cap or SMTP failure means an IP gets unlimited requests that each do the code upsert + advisory-lock tx + delete (and, on SMTP down, an SMTP connection attempt up to 30 s). Bounded by MAX_IN_FLIGHT but DB-write amplification during an outage/cap. Consider a separate loose per-IP counter for failures.
- L2 otp.rs:57-76: at cap or on failure, a start for a victim email whose live code is older than 60 s replaces then deletes that code (discard_code). Pre-existing for send failure, now also reachable at cap. Harmless at cap (no one can sign in), noted.
- L3 otp_budget.rs:11-13: a single global cap lets an attacker rotating IPs/emails exhaust the 450 slots and deny sign-in for everyone (inherent tradeoff; per-IP 20/h limits it to ~23 IPs per day-equivalent). Worth an alert on the "global OTP send cap reached" warn log.
- L4 deploy.sh:53: a relay-only host that predates .mode (no marker, relay-only compose) is not detected, so a plain deploy there would land the full stack. Only affects hosts deployed before this change; one-time `echo relay-only > .mode` fixes it.
- Also: unrecord (rate_limit.rs:146) pops the newest hit, which under concurrency may belong to another request from the same IP. Count is still correct.

## Unresolved
- None blocking. Below 9.5 threshold: fix M-A (message/label, 2 lines) and decide on M-B, then it clears.

**Status:** DONE_WITH_CONCERNS
**Summary:** All round-1 fixes verified correct; no critical/high defects, cms-node deploy and migration 0003 are safe. Score 9.3/10 (below 9.5 auto-merge threshold).
**Concerns/Blockers:** M-A misleading guard advice / project-label coupling; M-B ambiguous-timeout refund can understate sends.
