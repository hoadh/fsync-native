# Phase 04: Menu, device list, live status, trust-reset wiring

## Context Links
- docs/ffi-contract.md (`Device`, `on_device_status`, `AccountEvent`, trust-reset app guidance)
- plans/261014-1206-macos-trust-reset-prompt/plan.md; spec phase-11 trust-reset note

## Overview
Priority P1 · Status completed · 7h.
- The MenuBarExtra popover has a status header (LAN/relay/offline counts), the device list, a "Recent items" placeholder and actions (Add device…, Settings…, Quit).
- Remove device with a confirm dialog.
- Wire `TrustResetPromptModel` into the app.

## Key Insights
- `on_device_status(Device)` carries the full record. Merge it by `nodeId`. Call `devices()` again on `ListAdvanced`, after pairing and on popover open (cheap, and it fixes missed events).
- Trust reset:
  - `refresh()` runs after each successful `ensureAccount`.
  - `AccountEvent` goes to `handle` on main.
  - `outcome == .removedNeedsPairing` → phase `.needsPairing`.
  - `SelfRemoved` → `.needsPairing` as well.
- `SessionExpired` → clear the tokens → `.signedOut`.
- `HistoryReset{backup}` → an informational banner.
- `confirmationDialog` rendering inside MenuBarExtra is unverified. Show the trust-reset prompt and the remove confirm in the main Window; the popover only shows a "Review…" badge.

## Requirements
- DevicesModel: `devices` sorted with self first, then by name; `summary` (counts of lan/relay/offline, excluding self).
  - `remove(nodeId:)`: busy guard; on success reload.
  - Self has no remove action.
- DeviceRowView: platform icon, name (verbatim), status dot plus a label (LAN / Relay / Offline, from `status`), "last seen" for offline devices when `lastSeenMs > 0`.
- Remove asks for confirmation: "Remove ‘{name}’? It will stop syncing with this account and must be paired again." Destructive role; the default is Cancel.
- Trust reset: `TrustResetPromptView(model:)` shows in the main Window whenever `pending` is non-empty or the outcome is not `.unanswered`. Opening it brings the window forward.

## Architecture
```
EngineBridge+Events.receive:
  .deviceStatus(d)          → devicesModel.merge(d)
  .account(.listAdvanced)   → devicesModel.reload()
  .account(.trustReset)     → trustModel.handle(e) → openWindow("main")
  .account(.selfRemoved)    → phase = .needsPairing
  .account(.sessionExpired) → AppSecrets.clearSession; phase = .signedOut
  .account(.historyReset)   → banner
  .error(e)                 → banner(ErrorText)   (no phase change)
trustModel.$outcome sink: .removedNeedsPairing → phase .needsPairing
MenuBarContentView = StatusHeaderView + DeviceListView + actions
```
`StatusSummary` is a pure function over `[Device]`, unit tested.

## Related Code Files
Create:
- `Sources/FsyncMacCore/Features/Devices/DevicesModel.swift`
- `Sources/FsyncMacCore/Features/Devices/StatusSummary.swift`
- `Sources/FsyncMacUI/Menu/MenuBarContentView.swift`
- `Sources/FsyncMacUI/Menu/StatusHeaderView.swift`
- `Sources/FsyncMacUI/Devices/DeviceListView.swift`
- `Sources/FsyncMacUI/Devices/DeviceRowView.swift`
- `Tests/FsyncMacCoreTests/DevicesModelTests.swift`, `StatusSummaryTests.swift`, `EngineBridgeEventsTests.swift`

Modify:
- `EngineBridge.swift`: owns `devicesModel`, `trustModel`; calls `trustModel.refresh()` after ensureAccount
- `EngineBridge+Events.swift`: full routing above
- `RootView.swift`: trust-reset sheet, remove confirm
- `FsyncMacApp.swift`: menubar icon reflects status (any online peer: filled icon)

## FFI calls
`devices()`, `removeDevice(nodeId:)`, delegate `onDeviceStatus`/`onAccountEvent`/`onError`; through the TrustResetPrompt model: `pendingTrustResets`, `confirmTrustReset`, `declineTrustReset`.

## Implementation Steps
1. StatusSummary and DevicesModel (merge, sort, remove).
2. Complete the bridge routing and the trust-model outcome sink (`AnyCancellable` stored on the bridge).
3. Menu views. "Recent items" is a static placeholder section (phase 12 fills it).
4. RootView: trust-reset presentation and the remove confirmation.
5. Tests:
   - Merge updates the status of an existing device and appends an unknown one.
   - The summary excludes self.
   - Bridge `receive` with real `AccountEvent` values:
     - `.selfRemoved` → `.needsPairing`
     - `.sessionExpired` → `.signedOut` and the test Keychain session is cleared
     - `.trustReset` → the trust model refreshes (empty on a fresh engine)
   - `remove` offline → error text, list unchanged.

## Todo List
- [x] DevicesModel + StatusSummary + tests
- [x] Bridge event routing + trust outcome sink + tests
- [x] Menu / header / list / row views
- [x] Remove confirm + trust-reset presentation in main Window
- [ ] Manual: confirmationDialog renders (remove + trust reset) (needs a person at a Mac; phase 6)

## Success Criteria
- Unit tests green. Integration: the CLI peer shows Lan within 5 s of coming online, and removing it drops it from `devices()`.
- Manual check of both dialogs, recorded in the phase 6 report.

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| Status events missed during sleep or wake | M×L | Reload on popover open and on `NSWorkspace.didWakeNotification` |
| Dialog does not render in popover | M×M | All dialogs live in the main Window |
| Bridge file grows past 200 lines | M×L | Routing lives in `+Events`; sub-models own their logic |

## Security Considerations
- Removing a device signs a new list in the core. The app requires explicit confirmation, and self cannot be removed.
- Device names are untrusted text, so they are rendered verbatim and truncated.

## Next Steps
Phase 5: settings and app bundle.
