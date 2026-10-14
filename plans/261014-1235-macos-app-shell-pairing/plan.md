---
title: "macOS app shell: menubar, login, pairing, devices (phase 11)"
description: "SwiftPM FsyncMac app over FsyncCoreKit: EngineBridge, Keychain, OTP login, pairing with SAS, device list, trust-reset wiring, launch at login"
status: completed
priority: P1
effort: 42h
branch: macos/phase-11
tags: [macos, swiftui, ffi, pairing, keychain]
created: 2026-10-14
---

# macOS app shell + pairing (MVP phase 11)

Spec: [phase-11](../261013-2029-fsync-native-mvp/phase-11-macos-app-shell-pairing.md) · Contract: docs/ffi-contract.md (v2) · Pattern: [trust-reset prompt](../261014-1206-macos-trust-reset-prompt/plan.md)

## Decisions (locked)
- SwiftPM package `apps/macos/FsyncMac/` (no xcodeproj), tools 6.0 (Swift 6 strict concurrency), macOS 13.
  Targets: `FsyncMacCore` (logic, testable) → `FsyncMacUI` (SwiftUI views, render-testable) → `FsyncMac` (executable, `@main` only). Tests: `FsyncMacCoreTests`, `FsyncMacUITests`.
- Depends on `../Packages/FsyncCoreKit` (FsyncCoreKit, TrustResetPrompt). No Rust changes, nothing outside `apps/macos/**` except docs/plans.
- Engine access only through `EngineBridge` (@MainActor ObservableObject, `@Combine.Published`); blocking calls via one `EngineCall.run` helper (Task.detached). Callbacks hop to main via `DispatchQueue.main.async` (FIFO, keeps event order).
- Text entry (login, code join) lives in a regular `Window` scene, not the MenuBarExtra popover (focus risk).
- Errors: one pure `ErrorText.message(for:context:)` mapping; `Cancelled` is silent; `Auth`/`SessionExpired` route to sign-in.

## Phases
| # | Phase | Effort | Status |
|---|---|---|---|
| 1 | [Package, bridge, Keychain, boot](phase-01-package-bridge-keychain.md) | 10h | completed |
| 2 | [Onboarding: email, OTP, first device](phase-02-onboarding-login-otp.md) | 5h | completed |
| 3 | [Pairing: offer/QR, join, SAS](phase-03-pairing-offer-join-sas.md) | 8h | completed |
| 4 | [Menu, devices, status, trust-reset wiring](phase-04-menu-devices-trust-reset.md) | 7h | completed |
| 5 | [Settings, launch at login, .app bundle script](phase-05-settings-launch-at-login-bundle.md) | 4h | completed (manual checks open) |
| 6 | [Dev-backend integration test + docs](phase-06-integration-test-and-docs.md) | 8h | completed (manual checks open) |

## Dependencies
1 → 2 → 3 → 4 → 5 → 6, sequential. Phases 2-4 each extend `EngineBridge+Events.swift` and `RootView.swift`, so they cannot run in parallel. Unit tests are written inside each phase. Phase 6 adds only the backend run and docs.

## Test matrix
| Layer | What | How |
|---|---|---|
| Unit (pure) | AppPhase mapping, ErrorText, StatusSummary | XCTest, real FFI enum values |
| Unit (real deps) | KeychainStore, AppSecrets | real Keychain, service `app.fsync.mac.tests.<uuid>`, deleted in teardown |
| Model | Bridge boot, Login/Pairing/Devices models | real `FsyncEngine`, temp dir, api `http://127.0.0.1:9` (error paths) |
| Render | each view in key states | `ImageRenderer` → PNG (precedent: TrustResetPrompt) |
| Integration | login via OTP, first device, CLI pairs both directions with SAS match, status Lan ≤5 s, remove device | `apps/macos/scripts/run-fsync-mac-integration.sh` (docker pg + dev backend + relay + fsync-cli), test skipped without `FSYNC_IT_API` |
| Manual | MenuBarExtra, confirmationDialog (remove + trust reset), launch at login, Keychain prompts | `FsyncMac.app` from the bundle script |

Gates: `swift test` in FsyncMac and FsyncCoreKit pass with 0 warnings; integration script PASS; code-review ≥9.5, 0 critical. Status: all met (see Results).

## Rollback
The work is additive under `apps/macos/FsyncMac/` and `apps/macos/scripts/`. No core, backend or data migration is involved. Revert per phase commit. Dev Keychain items are removed with `security delete-generic-password -s app.fsync.mac` (see phase 1).

## Success criteria
- Fresh Mac: sign-in → "first device" (Created) → Ready. CLI pairs via code, SAS words equal on both sides, and the Mac must confirm. Device status reads Lan/Relay/Offline and changes within 5 s (asserted for online).
- Secrets exist only in the Keychain: tokens, device_secret, history_key. Nothing is written in plaintext under the data dir by the app.

## Results (2026-10-14)
- Review: 9.0 → 9.5 → 9.6, 0 critical. Report: `plans/reports/code-reviewer-261014-1340-macos-app-shell-review.md`.
- FsyncMac: 89 tests, 3 integration tests skipped by default (`FSYNC_IT_API` unset). FsyncCoreKit: 8 + 14.
- `run-fsync-mac-integration.sh`: PASS against dev backend, relay and CLI.
- Cargo: 253 passed, 1 ignored. Report: `plans/reports/tester-261014-1450-macos-app-shell-final-test-run.md`.
- Bundle: ad-hoc signed with runtime (hardened).

## Open items
Manual, unticked (phase 4, 5, 6 files):
- Launch window close on a real desktop.
- Settings window and launch-at-login toggle.
- Remove and trust-reset confirmationDialogs.
- Keychain prompts (verify on a signed build).

Carried over:
- Developer ID / signing decision.
- Core offline `logout()`: the refresh token stays valid server-side.
- Integration tests are not in CI (phase 10).
- Optional Low items from review round 3.

## Unresolved questions
1. Bundle id: `app.fsync.mac` is assumed. Production `api_base`/relay URLs are unknown (phase 10), so `Info.plist` keys come from script args and default to localhost dev.
2. Data-protection Keychain needs an entitlement, and an ad-hoc signature cannot carry it. On the file-based keychain `kSecAttrAccessible…ThisDeviceOnly` is not enforced. Plan: probe once and fall back (phase 1). Need to know whether a Developer ID / provisioning profile is planned.
3. Resolved: a Mac with no device to pair with gets a "Lost your other devices?" path in onboarding (phase 2) calling `reset_account_trust` behind a two-step destructive confirm (recovery = trust reset + re-pair, validation S1). End users have no CLI.
4. The integration script copies ~40 lines of the dev-backend setup from `tests/e2e/p0-exit-demo.sh`. Should the Rust owner extract `tests/e2e/lib-dev-backend.sh` to source?
5. Offline flip timing depends on core presence timeouts, so only the online flip is asserted.
