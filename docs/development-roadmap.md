# Development Roadmap

Last updated: 2026-10-14. Plan of record: [plans/261013-2029-fsync-native-mvp](../plans/261013-2029-fsync-native-mvp/plan.md). Its phase files still show "pending"; this page is the current status.

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
| 10 | Infra: relay, backend deploy, CI | Partial | Production stack in `deploy/` (compose, Caddy snippet, `deploy/scripts/`): backend and relay bound to loopback 18080/18081 behind host Caddy on a shared VPS. Backups, rollback and smoke check scripted. CI path-filtered jobs and `deploy-stack.yml` added; the GitHub workflows have not yet run on a hosted runner. Public HTTPS smoke and two-client e2e verified 2026-10-14. Open: second region, alerts, first GitHub run. |
| 11 | macOS app: shell, auth, pairing, devices | Done with concerns | `apps/macos/FsyncMac`: menubar, login, pairing, devices, recovery, trust-reset wiring, settings, bundle script. Integration run passes against a dev backend. Open: manual UI checks, signed build for the data-protection Keychain, Lan/Relay status needs phase 12 sync. |
| 12 | macOS app: clipboard, files, history, settings | Implemented; automated tests pass; manual QA open | Clipboard sync (auto/manual), privacy filters, history, settings, file clips, incoming offers, transfers, retention. Swift tests green; integration script 12/12 against a real backend and relay (run locally, needs Docker). Open: manual QA, copy-back of full history content. |
| 13 | Windows app: shell, auth, pairing, devices | Partial | `apps/windows`: WinUI 3 tray app, DPAPI secrets, sign-in, pairing (code/QR/SAS), devices, start with Windows; 35/35 unit tests; `tests/e2e/pair-with-cli.ps1` PASS against fsync-cli. Open: pairing with the Mac app, ARM64 hardware. |
| 14 | Windows app: clipboard, files, history, settings | Not started | Plan P1 w5-11. |
| 15 | Cross-OS integration and network test matrix | Not started | Harness depends on 10. |
| 16 | Release: signing, notarization, auto-update | Not started | Depends on 10, 12, 14. |

Phase names and dependencies follow the MVP plan. Its own status column still reads "pending" or "Partial"; the table above is the current state.

## Open Items (priority order)

1. ~~**Green gate on the final tree.**~~ Green 2026-10-14 11:48: fmt, clippy `-D warnings`, `cargo test --workspace` 253 pass / 0 fail, `swift test` 8/8, xcframework, e2e PASS. Open: the earlier `devices_reset` `PortNotExposed` failure did not recur, but its root cause is unverified (`backend/tests/common/pg.rs` retry is not selective).
2. **E2E repeat runs and relay-only stability.** The final tree has one e2e PASS. Target is 3 runs. An interim run once timed out on the relay-only round. Run the demo 2 more times and record the results.
3. ~~**Trust reset confirmation API (M5).**~~ Done 2026-10-14: `reset_account_trust`, `confirm_trust_reset`, `decline_trust_reset`, `pending_trust_resets` in core, FFI and CLI (ADR 012). The macOS `TrustResetPrompt` library shows the warning (2026-10-14); wiring it into the app is done (phase 11, 2026-10-14). Pending state is in memory only.
4. **Denylist enforcement.** The macOS app applies the denylist and sensitive-type filters to automatic sync (phase 12). Core still stores and validates the setting without enforcing it. Decide whether core enforces it too, for other clients.
5. **Two-machine measurements.** mDNS median (~2.7 s on one spike run), hole-punch ratio, real LAN results. Feeds ADR 007.
6. **Windows ARM64 + app e2e.** Run on ARM64 hardware, pair Windows with the Mac app (phase 11). Read-ahead in `read_file_range` landed in core (46 to 105 MiB/s at 50 ms RTT). Re-run the 200 MiB shell paste on Windows to confirm (ADR 010).
7. **x86_64 slice.** Execute at least once on an Intel host or in CI.
8. **Production stack (phase 10).** Stack runs on loopback behind host Caddy. Public HTTPS (`api-fsync.10z.one`, `relay-fsync.10z.one`) verified 2026-10-14. Open: first GitHub run of `ci.yml` and `deploy-stack.yml`; a second relay region.
9. **Backend production gaps.** Multi-instance state (presence, rate limits are in memory), Postgres TLS (sqlx `tls-rustls` not enabled), a real email provider (OTP is currently logged only; production mode cannot send codes yet).
10. **Logout is not a hard guarantee.** Logout revokes the session with background retry, but the server-side revoke is in memory and no `revoke_session` FFI exists yet. Stale sessions can survive a crash until the retry succeeds.
11. **macOS manual QA and copy-back.** Run the manual paste and privacy matrix (phase 04/12). History keeps 200-character previews, so copy-back works only for text stored in full; images and files need a core content accessor (`history_item_reps` proposal).
12. **Phases 05, 11, 13-16.** Per plan.

## Milestones Reached

| Date | Milestone |
|------|-----------|
| 2026-10-13 | Phases 01, 02 (single host), 06, 07, 09 landed; backend review rounds 1-2 and core 06/07 security fixes applied |
| 2026-10-14 | Phase 08 landed (facade, FFI v1, CLI, Swift package, e2e demo); core 08b review rounds 3-7 applied |
| 2026-10-14 | Track merge into local main (HEAD d3435e8): core hardening (read-ahead, logout revocation, `engine.lock`), phase 10 production stack on loopback, phases 04 and 12 (macOS clipboard, files, history, settings) |

See [project-changelog.md](./project-changelog.md) for details.
