# Development Roadmap

Last updated: 2026-10-16. Plan of record: [plans/261013-2029-fsync-native-mvp](../plans/261013-2029-fsync-native-mvp/plan.md). Its phase files still show "pending"; this page is the current status.

## Phase Status

| Phase | Scope | Status | Notes |
|-------|-------|--------|-------|
| 01 | Scaffold, workspace, ADRs | Done | ADR 001-012 (ADR 009 added with phase 04). |
| 02 | iroh spike | Done with concerns | ADR 007. Single host only. Two-machine LAN and hole-punch numbers open. |
| 03 | FFI bindings | Partial | Swift and C# (uniffi-bindgen-cs, x64) done, ADR 008. 9/9 C# contract tests, 10k-callback stress OK. ARM64 DLL builds; open: running on ARM64 hardware, x86_64 macOS slice never executed. |
| 04 | macOS clipboard spike | Done for automated part; manual matrix open | `spikes/macos-clipboard` self-test, ADR 009 (hybrid file clips: eager under 20 MiB, lazy above). Open: manual paste matrix in Finder, Mail, Messages, Slack, VS Code, Chrome; real password manager and Safari concealed captures. |
| 05 | Windows clipboard spike | Partial | `spikes/windows-clipboard`, ADR 010. Automated self-test and WinUI 3 tray smoke pass on Win11 x64. Open: manual app matrix, real password managers, Win10, ARM64 hardware. |
| 06 | Core identity, trust, transport, pairing | Done | SAS pairing, device-list chain pinning, presence session. |
| 07 | Core clip and file protocols | Done | `fsync/clip/1`, `fsync/file/1`. 1 GiB benchmark is `#[ignore]`. |
| 08 | History, facade, FFI v1, CLI, Swift package, e2e demo | Done | Part A: SQLCipher history, FSB1 blobs, settings. Part B: `FsyncService`, FFI, `fsync-cli`, `FsyncCoreKit`, `tests/e2e/p0-exit-demo.sh`. |
| 09 | Backend (auth, device registry, pairing, presence) | Done | Review rounds 1-2 applied. Docker and compose exist. |
| 10 | Infra: relay, backend deploy, CI | Partial | Production stack in `deploy/` (compose, Caddy snippet, `deploy/scripts/`): backend and relay bound to loopback 18080/18081 behind host Caddy on a shared VPS. Backups, rollback and smoke check scripted. Public HTTPS smoke and two-client e2e verified 2026-10-14. Second relay region (Singapore, relay-only mode) deployed 2026-10-15. Clients take both relays (`FSYNC_RELAY` / `--relay` comma list). 2026-10-15: hourly health probe, rollback guard, nightly encrypted off-box backups with a restore drill. Open: GitHub Actions blocked by billing (no CI, deploy or probe runs), first Actions run of both workflows, backup failure alert (log-only unless `BACKUP_PING_URL` is set). |
| 11 | macOS app: shell, auth, pairing, devices | Done with concerns | `apps/macos/FsyncMac`: menubar, login, pairing, devices, recovery, trust-reset wiring, settings, bundle script. Integration run passes against a dev backend. 2026-10-15: pending revoke in Keychain, retried at launch; comma-separated relay list. Open: manual UI checks, Lan/Relay status needs phase 12 sync. |
| 12 | macOS app: clipboard, files, history, settings | Implemented; automated tests pass; manual QA open | Clipboard sync (auto/manual), privacy filters, history, settings, file clips, incoming offers, transfers, retention. Swift tests green (222 run, 18 skipped); integration script against a real backend and relay (run locally, needs Docker). 2026-10-15: uses core `relay_urls`; copy-back of full history content via `history_item_reps`. Open: manual QA. |
| 13 | Windows app: shell, auth, pairing, devices | Partial | `apps/windows`: WinUI 3 tray app, DPAPI secrets, sign-in, pairing (code/QR/SAS), devices, start with Windows; `tests/e2e/pair-with-cli.ps1` PASS against fsync-cli. 2026-10-15: relay list in `FSYNC_RELAY`, DPAPI pending revoke, sign-out always completes locally, C# bindings regenerated. Open: pairing with the Mac app, ARM64 hardware. |
| 14 | Windows app: clipboard, files, history, settings | Merged 2026-10-15; manual GUI QA open | Clipboard sync, files, history, settings. Default hotkeys Win+Alt+C / Win+Alt+V. Windows 11 x64: `dotnet` 144/144 (includes the S5 relay and revoke work); 2026-10-15 round 2: History Copy with full content, 156/156 on merged main. Open: manual GUI QA. |
| 15 | Cross-OS integration and network test matrix | In progress | Netns matrix N1-N9 (`tests/e2e/netns`) runs in a privileged container: 8 PASS, n3 and n4 QUARANTINED (no direct path on the test relay; ~4 s first connect). Core hardening (offline start, fetch retry, redial stages, path tier, N8 refusal ack) merged. Cross-OS not started. |
| 16 | Release: signing, notarization, auto-update | Not started | Depends on 10, 12, 14. |

Phase names and dependencies follow the MVP plan. Its own status column still reads "pending" or "Partial"; the table above is the current state.

## Open Items (priority order)

1. ~~**Green gate on the final tree.**~~ Green 2026-10-14 11:48: fmt, clippy `-D warnings`, `cargo test --workspace` 253 pass / 0 fail, `swift test` 8/8, xcframework, e2e PASS. Open: the earlier `devices_reset` `PortNotExposed` failure did not recur, but its root cause is unverified (`backend/tests/common/pg.rs` retry is not selective).
2. **E2E repeat runs and relay-only stability.** The final tree has one e2e PASS. Target is 3 runs. An interim run once timed out on the relay-only round. 2026-10-15: `tests/e2e/run-matrix.sh` ran `p0` over `local,sg` (MIB=8), both PASS. Still open: repeat runs, and the `us` relay cell.
3. ~~**Trust reset confirmation API (M5).**~~ Done 2026-10-14: `reset_account_trust`, `confirm_trust_reset`, `decline_trust_reset`, `pending_trust_resets` in core, FFI and CLI (ADR 012). The macOS `TrustResetPrompt` library shows the warning (2026-10-14); wiring it into the app is done (phase 11, 2026-10-14). Pending state is in memory only.
4. **Denylist enforcement.** The macOS app applies the denylist and sensitive-type filters to automatic sync (phase 12). Core still stores and validates the setting without enforcing it. Decide whether core enforces it too, for other clients.
5. **Two-machine measurements.** mDNS median (~2.7 s on one spike run), hole-punch ratio, real LAN results. Feeds ADR 007.
6. **Windows ARM64 + app e2e.** Run on ARM64 hardware, pair Windows with the Mac app (phase 11). Read-ahead in `read_file_range` landed in core (46 to 105 MiB/s at 50 ms RTT). Re-run the 200 MiB shell paste on Windows to confirm (ADR 010).
7. **x86_64 slice.** Execute at least once on an Intel host or in CI.
8. **Production stack (phase 10).** Stack runs on loopback behind host Caddy. Public HTTPS (`api-fsync.10z.one`, `relay-fsync.10z.one`) verified 2026-10-14. ~~Second relay region~~ done 2026-10-15 (Singapore, `relay-sg-fsync.10z.one`, relay only). Open: first GitHub run of `ci.yml`, `deploy-stack.yml` and `health-probe.yml`; Actions is blocked by billing.
9. **Backend production gaps.** Multi-instance state (presence, rate limits are in memory), Postgres TLS (sqlx `tls-rustls` not enabled). ~~A real email provider~~ Gmail SMTP done 2026-10-15, with `OTP_DAILY_SEND_CAP` 450 per 24 h; a transactional provider is still recommended before real traffic.
10. ~~**Logout is not a hard guarantee.**~~ Resolved 2026-10-15: `logout_confirmed()` and engine-free `revoke_session` (FFI), plus a pending revoke kept by the macOS (Keychain) and Windows (DPAPI) apps and retried at launch. Still open: if the app is never launched again, the refresh token lives until its 30-day expiry. The core's background retry stays in memory; the backend revoke is persisted (`refresh_tokens.revoked_at`).
11. **Manual QA.** Run the manual paste and privacy matrix (phase 04/12) and the macOS and Windows GUI checks. Copy-back of full history content is done on macOS and Windows via `history_item_reps` (2026-10-15).
11a. **DKIM for `10z.one`.** Not found at common selectors. Enable it in Google Admin, publish the TXT record, then tighten DMARC from `p=none`.
11b. **Backup failure alert.** Set `BACKUP_PING_URL` (dead-man's switch) or accept log-only failures. Owner decision.
12. **Phases 05, 11, 13-16.** Per plan.

## Milestones Reached

| Date | Milestone |
|------|-----------|
| 2026-10-13 | Phases 01, 02 (single host), 06, 07, 09 landed; backend review rounds 1-2 and core 06/07 security fixes applied |
| 2026-10-14 | Phase 08 landed (facade, FFI v1, CLI, Swift package, e2e demo); core 08b review rounds 3-7 applied |
| 2026-10-14 | Track merge into local main (HEAD d3435e8): core hardening (read-ahead, logout revocation, `engine.lock`), phase 10 production stack on loopback, phases 04 and 12 (macOS clipboard, files, history, settings) |
| 2026-10-15 | OTP email over Gmail SMTP with a global send cap; relay-only deploy mode; Singapore relay live; Windows phase 14 merged |
| 2026-10-15 | Multiple relays in core and FFI (home relay by latency); `logout_confirmed` and `revoke_session`; pending revoke on macOS and Windows (main 461dd57) |
| 2026-10-15 | Hardening round 2 merged into local main (`3c405b1`): core redial after peer restart (~2 s), `history_item_reps` and typed `EngineAlreadyRunning`, macOS and Windows copy-back, `/health/otp`, health probe, rollback guard, off-box backups; rollback rehearsal incident (about 5 min API 502) |
| 2026-10-16 | Core hardening and netns matrix merged into local main (`026fb5a`): offline start from the cached list (7-day bound), fetch retry, staged redial cleanup, CLI `path` tier, clip/2 refusal ack (N8) |

See [project-changelog.md](./project-changelog.md) for details.
