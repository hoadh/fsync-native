---
owns: ["apps/macos/FsyncMac/Sources/FsyncMacUI/Devices/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/History/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/Transfers/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/Pairing/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/Onboarding/**", "apps/macos/FsyncMac/Tests/FsyncMacUITests/OnboardingRenderTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacUITests/PagesRenderTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacUITests/RenderTestSupport.swift", "apps/macos/Packages/FsyncCoreKit/Sources/TrustResetPrompt/**", "apps/macos/Packages/FsyncCoreKit/Tests/TrustResetPromptTests/**"]
dependsOn: [02]
---

# Phase 03: macOS pages (this Mac)

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` (copy, icons, state matrix, sanitizer, destructive dialog rule)
- Research: `research/researcher-261019-macos-native-ui-patterns.md` (Table, ContentUnavailableView, searchable, dropDestination, ImageRenderer limits)
- Current views: `apps/macos/FsyncMac/Sources/FsyncMacUI/{Devices,History,Pairing,Onboarding}/*.swift`, `ReadyView.swift` (layout reference; deleted in phase 04), `Menu/FilesMenuSection.swift`
- Phase 02 APIs: `FilesSync.canRetry/retry`, `TransferRow.revealURL`, `DropSendRequest.validate`

## Overview
- Priority: P1. Status: pending.
- Build the detail pages the shell (phase 04) hosts: Devices page, History page, Transfers page, pairing sheet, restyled onboarding gates. Pages compile and render standalone.
- Quick Look and `symbolEffect` moved to stretch phase 06. <!-- red-team #6 -->

## Key Insights
- Pages-before-shell keeps file ownership disjoint: phase 04 only references these public views.
- `ImageRenderer` renders `List`/`Table`/`NavigationSplitView` blank: use `NSHostingView` in an offscreen `NSWindow` for those.
- `OnboardingRenderTests.swift` builds a real `FsyncEngine` in a temp dir; extract it into `RenderTestSupport.swift` (DRY).
- `DeviceListView` / `DeviceRowView` are also used by the popover: keep a `compact` style parameter (DRY).
- `@FocusState` views (LoginView, OtpView) are skipped by ImageRenderer today; keep skipping.
- TrustResetPrompt already binds its confirmation dialog to the displayed resetId (`TrustResetMapping.dialogIsPresented`), and Return = "Keep current devices". `RecoverySheet` Return = Cancel. Keep both and assert them in tests. <!-- red-team #11 -->
- `TrustResetPendingView` renders `info.deviceNames` raw. FsyncCoreKit cannot import `PeerText` (FsyncMacCore), so add `TrustResetMapping.displayNames(_:)` implementing the spec sanitizer, tested with the same spec vectors. <!-- red-team #11 -->
- Phase 06 (stretch) may later edit `Transfers/TransfersTableView.swift` and `History/HistoryRowView.swift` (Quick Look, symbolEffect) under the documented handoff. Keep row action code in those two files so the handoff stays small. <!-- red-team #6 -->

## Requirements
- Devices page (`Devices/DevicesPageView.swift`):
  - header "Your devices" + count; `List` rows with platform symbol, name, status dot + LAN/Relay/Online/Offline, last seen.
  - Context + trailing menu: Send files… (NSOpenPanel), Remove… (`confirmationDialog`, Cancel default).
  - `.dropDestination(for: URL.self)` → `DropSendRequest.validate` → `DropSendConfirmation` sheet listing every file (name + size) with Send / Cancel (Cancel default) → `filesSync.send`. Rejections are shown as an inline message. <!-- red-team #13 -->
  - Empty state `ContentUnavailableView` "No other devices yet" + "Pair device…" (closure from shell).
- History page (`History/HistoryView.swift` restyle): `.searchable` (keep the 250 ms debounce), 2-line rows, toolbar Clear… menu, row actions Copy / Pin / Delete, Load more, empty + no-results states. Remove the fixed min size. No refresh call in the view (bridge refreshes on navigation, phase 02). <!-- red-team #14 -->
- Transfers page (`Transfers/TransfersPageView.swift`, `TransferOfferSection.swift`, `TransfersTableView.swift`, `SendFilesMenu.swift`):
  - Offers section: sender (sanitized), count, size, "Save to Downloads" / Decline.
  - `Table` columns: Name, Direction, Size, Progress (plain ProgressView), Status.
  - Row actions: Cancel (active), Retry (enabled only when `canRetry`; disabled while in flight), Show in Finder (`revealURL`).
  - Toolbar: Send files…, Clear finished. Empty state "No transfers yet". `filesSync.message` as a banner.
- Pairing sheet (`Pairing/PairingSheet.swift`): wraps `PairingFlowView(model:role:.join)`; Cancel = `model.cancel` + dismiss; Done on `.done`; width ~420. QR + existing countdown unchanged.
- Onboarding: LoginFlowView, NeedsPairingView, FirstDeviceView, RecoverySheet restyled per spec; behavior and default buttons unchanged.
- TrustResetPrompt: `displayNames` sanitizer used by `TrustResetPendingView`; spacing/header only if the spec lists deltas; copy unchanged.
- Non-functional: files < 200 lines; VoiceOver labels on icon-only buttons; Esc = cancel in sheets.

## Architecture
```
Shell (04) ─> DevicesPageView(devices, files, onPair) ─ drop → DropSendRequest → DropSendConfirmation → files.send
          ├─> HistoryView(history, devices, sync)
          ├─> TransfersPageView(transfers, files, devices)
          └─> .sheet PairingSheet(pairing, onClose)
Models from FsyncMacCore (ObservableObject). Views call model methods; no engine access.
```

## Related Code Files
- Modify: `Sources/FsyncMacUI/Devices/{DeviceListView,DeviceRowView,FilePicker}.swift`, `History/{HistoryView,HistoryRowView}.swift`, `Pairing/{PairingFlowView,PairOfferView,PairJoinView,SasConfirmView}.swift` (spacing), `Onboarding/{LoginView,OtpView,NeedsPairingView,RecoverySheet,FirstDeviceView}.swift`, `Tests/FsyncMacUITests/OnboardingRenderTests.swift`, `Packages/FsyncCoreKit/Sources/TrustResetPrompt/{TrustResetMapping,TrustResetPendingView}.swift` (+ optional `TrustResetOutcomeView`, `TrustResetPromptView`), `Packages/FsyncCoreKit/Tests/TrustResetPromptTests/*`
- Create: `Devices/DevicesPageView.swift`, `Devices/DropSendConfirmation.swift`, `Transfers/{TransfersPageView,TransferOfferSection,TransfersTableView,SendFilesMenu}.swift`, `Pairing/PairingSheet.swift`, `History/HistoryEmptyState.swift` (if needed), `Tests/FsyncMacUITests/{RenderTestSupport,PagesRenderTests}.swift`, `Packages/FsyncCoreKit/Tests/TrustResetPromptTests/TrustResetDisplayNamesTests.swift`
- Delete: none (`ReadyView.swift` deleted in phase 04)

## Implementation Steps
1. Extract fixtures into `RenderTestSupport.swift` (+ `hostOffscreen(_:size:)`).
2. Devices page + compact row + drop → validate → confirmation sheet + empty state.
3. History restyle + states.
4. Transfers page (offers, table, Cancel/Retry/Show in Finder, toolbar, empty state).
5. Pairing sheet.
6. Onboarding + TrustResetPrompt restyle; `displayNames` + tests (spec vectors; dialog id binding; Return = Keep).
7. `PagesRenderTests`: each page empty + populated offscreen. Transfers has one active, one failed-retryable and one done row. Spec icon map symbols resolve via `NSImage(systemSymbolName:)`.
8. `swift build && swift test` in FsyncMac; `swift test` in FsyncCoreKit.

## Todo List
- [ ] RenderTestSupport
- [ ] DevicesPageView + drop confirmation + empty state
- [ ] HistoryView restyle + states
- [ ] TransfersPageView (offers, table, Cancel/Retry/Show in Finder/Clear finished, Send files…)
- [ ] PairingSheet
- [ ] Onboarding restyle
- [ ] TrustResetPrompt displayNames sanitizer + tests
- [ ] PagesRenderTests + symbol test

## Success Criteria
- `cd apps/macos/FsyncMac && swift build && swift test` green; new render tests run (not skipped).
- `cd apps/macos/Packages/FsyncCoreKit && swift test` green (existing 14 TrustResetPrompt + 8 core + new displayNames tests).
- `grep -rn quickLookPreview apps/macos/FsyncMac/Sources` empty (stretch only).
- App still launches with the old shell (`swift run FsyncMac`).

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| `Table` row actions awkward | M x M | Context menu + selection toolbar; fall back to `List` |
| Offscreen hosting flaky in CI | M x M | Assert size/no-crash only |
| Device row change breaks popover | M x L | `compact` defaults to the current look |
| Drop sends unexpected files | L x H | Validation + full-list confirmation, Cancel default |

## Security Considerations
- Show in Finder only on engine-reported `row.path`; never build paths from peer names.
- All peer names sanitized (PeerText / displayNames). Drop: file URLs only, symlinks resolved, user confirms the full list.

## Next Steps
- Phase 04 wires pages into the split view. Rollback: revert phase commits; the old shell keeps working (`ReadyView` and the history window are untouched here).
