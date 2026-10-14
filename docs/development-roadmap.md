# Development Roadmap

Last updated: 2026-10-14. Plan of record: [plans/261013-2029-fsync-native-mvp](../plans/261013-2029-fsync-native-mvp/plan.md). Its phase files still show "pending"; this page is the current status.

## Phase Status

| Phase | Scope | Status | Notes |
|-------|-------|--------|-------|
| 01 | Scaffold, workspace, ADRs | Done | ADR 001-008, 011, 012. ADR 009/010 unused. |
| 02 | iroh spike | Done with concerns | ADR 007. Single host only. Two-machine LAN and hole-punch numbers open. |
| 03 | FFI bindings | Partial | macOS/Swift done (ADR 008). Windows C# half open: `uniffi-bindgen-cs`, DLL, 10k callback stress. x86_64 slice never executed. |
| 04 | macOS clipboard spike | Not started | No spike directory or ADR. |
| 05 | Windows clipboard spike | Not started | |
| 06 | Core identity, trust, transport, pairing | Done | SAS pairing, device-list chain pinning, presence session. |
| 07 | Core clip and file protocols | Done | `fsync/clip/1`, `fsync/file/1`. 1 GiB benchmark is `#[ignore]`. |
| 08 | History, facade, FFI v1, CLI, Swift package, e2e demo | Done | Part A: SQLCipher history, FSB1 blobs, settings. Part B: `FsyncService`, FFI, `fsync-cli`, `FsyncCoreKit`, `tests/e2e/p0-exit-demo.sh`. |
| 09 | Backend (auth, device registry, pairing, presence) | Done | Review rounds 1-2 applied. Docker and compose exist. |
| 10 | Infra: relay, backend deploy, CI | Partial | CI (`.github/workflows/ci.yml`) and backend Dockerfile/compose exist. `deploy/relay/` is empty. No 2-region relay. |
| 11 | macOS app: shell, auth, pairing, devices | Done with concerns | `apps/macos/FsyncMac`: menubar, login, pairing, devices, recovery, trust-reset wiring, settings, bundle script. Integration run passes against a dev backend. Open: manual UI checks, signed build for the data-protection Keychain, Lan/Relay status needs phase 12 sync. |
| 12 | macOS app: clipboard, files, history, settings | Not started | Plan P1 w5-11. |
| 13 | Windows app: shell, auth, pairing, devices | Not started | Plan P1 w1-5. Depends on 03, 05, 08, 09. |
| 14 | Windows app: clipboard, files, history, settings | Not started | Plan P1 w5-11. |
| 15 | Cross-OS integration and network test matrix | Not started | Harness depends on 10. |
| 16 | Release: signing, notarization, auto-update | Not started | Depends on 10, 12, 14. |

Phase names and dependencies follow the MVP plan. Its own status column still reads "pending" or "Partial"; the table above is the current state.

## Open Items (priority order)

1. ~~**Green gate on the final tree.**~~ Green 2026-10-14 11:48: fmt, clippy `-D warnings`, `cargo test --workspace` 253 pass / 0 fail, `swift test` 8/8, xcframework, e2e PASS. Open: the earlier `devices_reset` `PortNotExposed` failure did not recur, but its root cause is unverified (`backend/tests/common/pg.rs` retry is not selective).
2. **E2E repeat runs and relay-only stability.** The final tree has one e2e PASS. Target is 3 runs. An interim run once timed out on the relay-only round. Run the demo 2 more times and record the results.
3. ~~**Trust reset confirmation API (M5).**~~ Done 2026-10-14: `reset_account_trust`, `confirm_trust_reset`, `decline_trust_reset`, `pending_trust_resets` in core, FFI and CLI (ADR 012). The macOS `TrustResetPrompt` library shows the warning (2026-10-14); wiring it into the app is done (phase 11, 2026-10-14). Pending state is in memory only.
4. **Denylist decision.** Setting is stored and validated but not enforced in core. Enforce in the clip path or drop the setting from the UI plan.
5. **Two-machine measurements.** mDNS median (~2.7 s on one spike run), hole-punch ratio, real LAN results. Feeds ADR 007.
6. **Windows phase 03 half.** `uniffi-bindgen-cs`, DLL packaging, callback stress test (10k).
7. **x86_64 slice.** Execute at least once on an Intel host or in CI.
8. **Relay deployment (phase 10).** Populate `deploy/relay/`, deploy a second region.
9. **Backend production gaps.** Multi-instance state (presence, rate limits are in memory), Postgres TLS (sqlx `tls-rustls` not enabled), a real email provider (OTP is currently logged only).
10. **Phases 04, 05, 11-16.** Per plan.

## Milestones Reached

| Date | Milestone |
|------|-----------|
| 2026-10-13 | Phases 01, 02 (single host), 06, 07, 09 landed; backend review rounds 1-2 and core 06/07 security fixes applied |
| 2026-10-14 | Phase 08 landed (facade, FFI v1, CLI, Swift package, e2e demo); core 08b review rounds 3-7 applied |

See [project-changelog.md](./project-changelog.md) for details.
