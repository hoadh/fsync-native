# Phase 11 — macOS app: menubar shell, login, pairing, device list

## Context Links
- Design §2, §4 flows 1-2; [phase-03](phase-03-spike-ffi-bindings.md) contract; [phase-04](phase-04-spike-macos-clipboard-lazy-paste.md); [phase-09](phase-09-backend-auth-registry-presence.md)

## Plan
- [macOS app shell + pairing](../261014-1235-macos-app-shell-pairing/plan.md) (sync 2026-10-14)

## Overview
- Priority: P1 | Status: Done on Mac (manual visual checks open; Mac↔Win CLI pairing PASS via Track X, Tailscale; app-to-app run open, 2026-10-14) | Effort: 4-5 wk (C) — parallel with phase 13
- SwiftUI `MenuBarExtra` app (LSUIElement), FsyncCoreKit Swift package wrapping generated bindings, login (email OTP), pairing UI (show QR/code, scan/enter code, SAS confirm), devices with status, launch at login.

## Key Insights
- Start w1 against contract v0 using CI xcframework; switch to real core as phase 08 lands (same API).
- Delegate callbacks arrive off-main → single `EngineBridge` (`@MainActor` ObservableObject) re-publishes state; views never touch FFI directly.
- QR scan on Mac impractical → Mac mostly *shows* QR; joins via 6-digit code. (Windows same; QR useful for future mobile.)

## Requirements
- Menubar popover: status header (devices online LAN/relay/offline), recent items placeholder, actions; Settings window; onboarding (login → pair or "first device").
- Tokens + device secret key + history key (32 B, generated on first launch) in Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`). <!-- Updated: Validation Session 1 - history_key -->
- Launch at login via `SMAppService.mainApp`.

## Architecture
```
apps/macos/
  FsyncMac.xcodeproj
  FsyncMac/
    App/FsyncMacApp.swift, App/AppDelegate.swift
    Bridge/EngineBridge.swift, Bridge/EngineDelegateAdapter.swift, Bridge/KeychainStore.swift
    Features/Onboarding/{LoginView, OtpView, FirstDeviceView}.swift
    Features/Pairing/{PairOfferView, PairJoinView, SasConfirmView}.swift
    Features/Devices/{DeviceListView, DeviceRowView}.swift
    Features/Menu/{MenuBarContentView, StatusHeaderView}.swift
    Settings/SettingsWindow.swift
  Packages/FsyncCoreKit/ (Package.swift, Sources/FsyncCoreFFI generated, binaryTarget xcframework)
```
Data flow: View → EngineBridge (async Task) → FsyncEngine (FFI) → core; core events → EngineDelegateAdapter → MainActor → @Published → View.

## Related Code Files
- Create: all above. Modify: none outside `apps/macos/**`.

## Implementation Steps
1. Project, MenuBarExtra, LSUIElement, FsyncCoreKit package consuming CI artifact.
2. EngineBridge + Keychain; engine start on launch with stored key/session.
3. Login/OTP screens → `set_session`; token refresh handled in core api_client.
4. Pairing offer (QR image via CoreImage + code) and join (code entry) + SAS confirm.
5. Device list w/ live status + remove device (confirm dialog).
6. Launch at login; basic settings shell.
7. XCTest for bridge state mapping; UI smoke test.

## Todo List
- [x] Project + FsyncCoreKit (SwiftPM, not xcodeproj)
- [x] EngineBridge + Keychain
- [x] Login/OTP
- [x] Pairing offer/join/SAS
- [x] Device list + remove
- [x] Launch at login (toggle and approval still a manual check)
- [x] Tests (FsyncMac 89 incl. 3 integration, skipped by default; FsyncCoreKit 8+14)

## Deviations (as built)
- SwiftPM package `apps/macos/FsyncMac/`, not `FsyncMac.xcodeproj`. The bundle comes from `apps/macos/scripts/build-fsync-mac-app.sh`.
- Status shows "Online" (presence) until phase 12 connects peers. The core reports Lan/Relay only for a live QUIC connection. The integration test asserts `online` within 5 s, not `.lan`.
- Keychain: the data-protection store needs a signed build (ad-hoc gives -34018), so it falls back to the file keychain with `AfterFirstUnlockThisDeviceOnly`. Migration to the data-protection store is untested until a signed build.
- Added: "Lost your other devices?" recovery (`reset_account_trust`, two-step confirm), per validation S1.
- Removed: "Recent items" popover placeholder (dead UI until phase 12).

## Success Criteria
- Fresh Mac: login → first device; second Mac/Win (CLI ok) pairs via code with SAS match; status flips LAN/relay/offline within 5 s of change.
- 2026-10-14: pairing part met via Track X (Mac↔Win, Tailscale, code+SAS). Status flip timing not verified in Track X; LAN path unreachable.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Core not ready w1 | M x M | build on contract v0 stub artifact |
| MenuBarExtra limitations (focus, text fields) | M x L | NSPopover via AppKit host if needed |

## Rollback
- App-only phase; feature flags not needed pre-release; revert commits.

## Security Considerations
- Keychain only for secrets; SAS confirmation mandatory before signing; remove-device requires confirmation; hardened runtime from day 1.

## Next Steps
- Phase 12 adds clipboard/files/history on this shell.

## Note: trust-reset prompt (library ready)
- `TrustResetPrompt` library exists in `apps/macos/Packages/FsyncCoreKit` (see `plans/261014-1206-macos-trust-reset-prompt/plan.md`).
- Wire: `EngineDelegateAdapter` → `model.handle(event)` on the main actor; route `.removedNeedsPairing` to the pairing flow.
- Unverified: `confirmationDialog` rendering. Check manually in the app.
