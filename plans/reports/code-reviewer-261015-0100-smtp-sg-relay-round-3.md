# Round 3 review: infra/smtp-sg-relay (bed3de2..c3c4f7f)
Score 9.6/10. Critical 0, High 0, Medium 0, Low 4.
Gates: fmt OK, clippy -D warnings OK, cargo test -p fsync-backend all pass (smtp_mailpit 13.0 s, was 30 s), shellcheck -S warning clean.

## M-A (project name in guard): fixed
proj = remote .env COMPOSE_PROJECT_NAME, else --env-file, default fsync; matches compose `name:` (compose reads .env in project dir). A fresh --dir with its own project name is no longer blocked; same-name second dir is still blocked; die message now correct. Guard still runs before any mutation.

## M-B (ambiguous timeout): fixed
- EmailError{msg,maybe_delivered}; smtp.rs classify uses is_timeout, overall tokio timeout is ambiguous. Ambiguous: code row, global slot, per-email and per-IP all kept. Definite: slot released, code deleted, per-email + per-IP unrecorded. Cap refusal: refunds per-email/per-IP (record-before-send then unrecord is balanced).
- SEND_TIMEOUT 12 s < REQUEST_TIMEOUT 15 s (limits.rs:12): correct reasoning, otherwise 408 and no classification. A handler cancelled anyway (slow DB + 12 s send) leaves everything spent, which is the conservative direction. Per-email record moved before send: right, a cancelled request still counts. No race introduced (peek earlier, record before await).
- Test is real (silent TcpListener, real HTTP path + direct sender in parallel), asserts 500, code row 1, send_log 1, per-email spent, elapsed <14 s. Runtime ~13 s.
- Docs (api.md 12 s, deployment-guide alert line, README .mode note) accurate.

## Low
- L1 smtp.rs classify: is_timeout also covers connect timeouts (nothing sent) and lettre socket timeouts before DATA. These are treated ambiguous, so with a black-holed SMTP host every attempt spends the global slot, per-email and per-IP and keeps a code row. During such an outage 450 attempts exhaust the 24 h cap with zero mail delivered (sign-in refused until window frees; rows are not released). Conservative and alertable (cap log), acceptable now; refine later by treating connect-phase timeouts as definite if lettre exposes it.
- L2 otp.rs ambiguous path: the kept code blocks a legitimate re-request for 60 s (RESEND_AFTER_MS) and the user sees 500; if the mail was not actually sent they must wait. Minor UX.
- L3 deploy.sh proj parse: value taken raw (no quote stripping) and interpolated unquoted into the remote command. A quoted `COMPOSE_PROJECT_NAME="fsync"` fails open for a fresh dir only (label mismatch, guard does not block; the same-dir compose.yml grep still works). Content comes from the operator's own .env, so no trust boundary; strip quotes + validate `[A-Za-z0-9_-]+` if tightening.
- L4 test timing margin: elapsed <14 s vs 12 s timeout on a loaded CI box could flake; consider 14->15 or asserting only the 500.

No regressions found in the OTP flow (refund/keep matrix verified in auth_otp_budget tests + new smtp test) or the deploy guard.

**Status:** DONE
**Summary:** M-A and M-B correctly fixed, extra changes (12 s timeout, record-before-send, docs) are sound; 9.6/10, 0 critical, above the 9.5 threshold.
**Concerns/Blockers:** None blocking. L1: black-holed SMTP burns the global cap without delivering (conservative by design, alert on cap log).
