# Track D infra round 2 report

Branch infra/round-2 (worktree /Users/alex/Code/fsync-native-wt/infra-round-2), based on main 2d87845, pushed to origin, NOT merged.
Commits: bd99b18 (D6), f4e867e (D2), e513c3e (D3), 549cc6a (D1), 6358db5 (D4), 08b1d55 (D7 + runbook), f5ea544, 3c80d23, 73de10c (review fixes).
Files: backend/{src/lib.rs,src/otp_budget.rs,tests/**}, deploy/**, .github/workflows/{ci,health-probe}.yml, tests/e2e/run-matrix.sh.
Gates: fmt ok; clippy -D warnings ok; cargo test --workspace: 301 passed, 0 failed, 2 ignored; shellcheck clean. Review: code-reviewer 9.5/10, 0 critical/high/medium (earlier 7 -> 8.5 -> 9 -> 9.5).

## D1 CI
- rust job failure was `ld` SIGBUS (disk/debuginfo) linking core test binaries. Fix: debuginfo off + free-disk step in ci.yml (549cc6a).
- Run 38630595475: rust SUCCESS. Final dispatch 38635166526 (https://github.com/hoadh/fsync-native/actions/runs/38635166526): rust SUCCESS, macOS FAILURE (app code), windows still running at report time (known app-code failure).
- macOS FAIL (app code, NOT fixed): Swift 6 "non-sendable result type '[String : Any]'" at apps/macos/FsyncMac/Tests/FsyncMacCoreTests/Integration/IntegrationSupport.swift:71,73 and FsyncMacIntegrationTests.swift:44,66,77,92 (cliLogin / waitEvent).
- Windows FAIL (app code, NOT fixed): Fsync.App.Tests.Round2Tests.EagerBudgetCoversStartingTheTransfers (Round2Tests.cs:75) expected TimeoutException, got FsyncException.Invalid "engine not started".

## D2 alerts
- Backend: GET /health/otp -> 200 "ok" / 503 "otp cap reached" (test in auth_otp_budget.rs). Existing WARN log on cap kept.
- deploy/scripts/probe-health.sh: api /health, /health/otp (404 = warn only, old releases), relay + relay-sg /healthz, cert expiry >= 14d, retries. .github/workflows/health-probe.yml hourly (17 * * * *) + dispatch; failure = red run = GitHub notification. Only verifiable after merge to default branch. OTP-cap alert repeats hourly until cleared (intentional).

## D3 rollback rehearsal (cms-node, 519ec821e90f)
- deploy.sh --rollback swaps compose.yml/.prev. Rehearsal 1 (to d9e5e9b7ef77) FAILED: old release lacks migration 0003, sqlx "migration 3 previously applied but missing", backend crash-looped, api 502 about 5 min (03:08-03:13 local) until rolled forward. Not silent: this was my rehearsal.
- Fix: deploy/scripts/rollback-guard.sh + --force-rollback; guard refuses targets whose migrations lack DB max version (verified refuses d9e5e9b7ef77: db 3 vs knows 2; unknown sha).
- Valid rehearsal: forward-deployed 549cc6a3d298 (branch build), --rollback to 519ec821e90f, --rollback forward. ~88 s each; external 0.5 s polling: 1 failed poll per hop. Smoke ok each hop.
- Steps: ssh ubuntu@cms-node; cd /opt/fsync; ./scripts/deploy.sh --rollback (refuses unsafe); run again to roll forward. Documented in deploy/operations.md.
- cms-node NOW runs 549cc6a3d298 (FSYNC_TAG_PREV=519ec821e90f). Redeploy from main after merge. Caddy untouched.

## D4 off-box backups
- Nightly 03:15 (cron /etc/cron.d/fsync-backup): pg_dump -> gpg encrypt (key fp 3634F25D6EBDA59944AF8FCBEF63544FE935E353, public key only on cms-node) -> sftp to SG fsyncbak (upload-only, restricted internal-sftp) -> ingest cron on SG every 10 min moves into store/ (ubuntu 700), retention 30 d, min keep 3, rejects future/bad names, never prunes when pipeline dead. Local 14 d.
- Private key: Mac ~/.fsync-backup/fsync-backup-secret.asc (600). OWNER MUST copy to a password manager.
- Evidence: several shipments, prune test, key restriction test, ingest tests on SG host. Restore drill on Mac in throwaway postgres:17 containers (removed): decrypt ok, pg_restore ok, counts match live (accounts 1, device_lists 1, history 2, refresh_tokens 2, migrations 3).
- Failure visibility: backup.log only, unless owner sets BACKUP_PING_URL (secret) in /opt/fsync/.backup/offsite.env.

## D5 mail auth for 10z.one (read-only, dig @1.1.1.1)
- Sender hi@10z.one via smtp.gmail.com. MX Google Workspace. SPF `v=spf1 include:spf.efwd.registrar-servers.com include:_spf.google.com ~all` (ok). DMARC `p=none` with Cloudflare rua. DKIM: none found at common selectors (google, default, selector1/2, k1, mail, dkim, s1, s2, mandrill) -> likely not enabled.
- Owner actions: enable DKIM in Google Admin, publish google._domainkey TXT, review DMARC reports, then p=none -> quarantine/reject. DMARC passes today via SPF alignment only.

## D6 flaky smtp_mailpit
- backend/tests/common/mailpit.rs: wait_ready (SMTP 220 greeting + HTTP info, 120 s), messages_to poll 20 s, no_mail_to for negative asserts. 8/8 pass; full workspace passes.

## D7 e2e matrix
- tests/e2e/run-matrix.sh: scenarios p0, stack; relays local/us/sg/URL; MIB; CELL_TIMEOUT_S watchdog; INT/TERM forwarded; summary table; `--list`. Real run MIB=8 p0 local,sg: both PASS (local 239 s, sg 51 s, sg ~0.38 MiB/s, clip latency 0.53 s). Watchdog and signal forwarding verified, no orphans. Runbook deploy/operations.md.

## Unresolved questions
- Owner: BACKUP_PING_URL (dead-man's switch) or accept log-only failure visibility?
- Apps track: fix macOS Swift 6 Sendable test errors and Windows Round2Tests failure.
- Lead: redeploy prod from main after merge (cms-node on branch build 549cc6a3d298).
- Hourly probe cadence vs Actions minutes (~720/2000 private); workflow verifiable only post-merge.
- Optional hardening (reviewer lows): SIGKILL escalation in matrix trap; disk quota on SG upload area.
