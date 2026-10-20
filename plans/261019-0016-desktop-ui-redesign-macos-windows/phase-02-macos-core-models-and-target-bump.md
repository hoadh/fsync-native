---
owns: ["apps/macos/FsyncMac/Package.swift", "apps/macos/scripts/build-fsync-mac-app.sh", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/FileFetchService.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/FilesSync.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/TransfersModel.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/IncomingFilesNotice.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/IncomingFilesNoticeCoalescer.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Files/DropSendRequest.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Features/Navigation/MainNavigation.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Bridge/EngineBridge.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Bridge/EngineBridge+Clipboard.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Bridge/EngineBridge+Models.swift", "apps/macos/FsyncMac/Sources/FsyncMacCore/Bridge/EngineBridge+Navigation.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/MainNavigationTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/TransfersRetryTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/IncomingFilesNoticeTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/DropSendRequestTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/EngineBridgeNoticeWiringTests.swift"]
dependsOn: [01]
---

# Phase 02: macOS core models + macOS 14 target (this Mac)

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` (notification rules, sanitizer, state matrix)
- `apps/macos/FsyncMac/Package.swift`, `Sources/FsyncMacCore/Files/{TransfersModel,FileFetchService,FilesSync}.swift`, `Sources/FsyncMacCore/Bridge/{EngineBridge,EngineBridge+Events,EngineBridge+Clipboard,EngineBridge+Models}.swift`, `Features/Devices/{PeerText,DevicesModel}.swift`
- Windows reference for Retry: `apps/windows/src/Fsync.App.Core/Files/TransfersViewModel.Pulls.cs`

## Overview
- Priority: P1. Status: completed (commit 06b5cc6, pushed to origin).
- Headless groundwork: deployment target bump, navigation state, Transfers Retry/Reveal with guards, incoming-files notice stream (sanitized, coalesced, ordered), drop validation, history refresh on navigation. No SwiftUI. Unit-tested.
- Moved out to stretch phase 06: `MenuBarIconState` and the bridge `menuBarIconState`. <!-- red-team #6 #8 -->

## Key Insights
- Ordering core → pages → shell → notifications lets each macOS phase compile alone and be reverted alone.
- `TransfersModel` members today: `rows`, `active` (exists), `handle`, `setName`, `outcome(of:)`, `waitOutcome`, `cancel`, `clearFinished`, `reset`. New: `finished`, `remove(_:)`. <!-- red-team #13 -->
- `List(selection:)` needs a `Binding<MainSection?>`. Decision: `MainNavigation.section: MainSection?` with default `.devices`. The detail view maps `nil` to `.devices`, and `show(_:)` always sets a non-nil value. Recorded in the spec. <!-- red-team #7 -->
- Retry spec storage lives in `FileFetchService`, and `canRetry` is computed in `FilesSync`, so `TransfersModel` stays independent of the fetcher.
- Notice ordering in `FilesSync.receive(offer:)` <!-- red-team #10 -->: compute `isNew = !offers.contains { $0.itemId == offer.itemId } && !recentNoticeIds.contains(offer.itemId)` BEFORE the existing `offers.removeAll`. If new, emit the notice (auto-accept branch: emit `autoAccepted: true` BEFORE `await accept(offer)`, so a long download does not delay it). The existing removeAll/insert logic is unchanged.
- History refresh: today `FsyncMacApp.showHistory()` calls `bridge.refreshHistory()` because a closed window keeps its view alive. With one window, the bridge subscribes to `navigation.$section`. Any assignment of `.history` (including a re-`show(.history)`) triggers `refreshHistory()`. <!-- red-team #14 -->
- Stored properties cannot live in extensions: add `let navigation` and `let incomingFiles` to `EngineBridge.swift` (182 lines; stays < 200). Other new members go in `EngineBridge+Navigation.swift`.

## Requirements
- `Package.swift` `.macOS(.v14)`; `build-fsync-mac-app.sh` `LSMinimumSystemVersion` `14.0`. FsyncCoreKit stays `.v13`.
- `MainNavigation` (`@MainActor ObservableObject`): `@Published section: MainSection? = .devices` (`devices`, `history`, `transfers`), `isPairingSheetPresented`, `show(_:)`, `presentPairing()`, `dismissPairing()`, `reset()`. <!-- red-team #7 -->
- Bridge: `navigation`, `incomingFiles: PassthroughSubject<IncomingFilesNotice, Never>`, a section sink → `refreshHistory()` on `.history`, and `navigation.reset()` on sign-out / phase change.
- Retry <!-- red-team #13 -->:
  - `FileFetchService` keeps `RetrySpec(itemId, file, destDir, peerNodeId)` keyed by transfer id, bounded (4 x maxRows).
  - `FilesSync.canRetry(_ row:) -> Bool` = failed receive && spec exists && spec.peerNodeId is in the current trusted device list && not in flight.
  - `FilesSync.retry(_ row:) async`: an in-flight guard keyed by `(itemId, fileId)` drops duplicate taps; it calls `TransfersModel.remove(row)` then `fetcher.fetch` (new id).
  - Specs are cleared on: device removal (bridge sink on `devicesModel.$devices` prunes specs whose peer left), trust-reset outcome (trusted/removed), sign-out, any phase change away from `.ready`.
- `TransferRow.revealURL: URL?` (done + path exists).
- `IncomingFilesNotice { fromName, fileCount, totalBytes, autoAccepted, offerCount }` + `title` / `body` per spec. `fromName` goes through `PeerText.sanitize` (spec vectors). Never file names. <!-- red-team #10 -->
- `IncomingFilesNoticeCoalescer` (pure, injectable clock): per sanitized sender, at most one emit per 10 s. Offers inside the window bump `offerCount` and are emitted at window end as one updated notice. Tests use a fake clock. <!-- red-team #10 -->
- `DropSendRequest.validate(_ urls: [URL]) -> Result<[URL], DropError>` <!-- red-team #13 -->: file URLs only (reject non-file schemes), `resolvingSymlinksInPath()`, reject directories that do not exist, dedupe. Phase 03 shows the returned full list for confirmation before send.
- Non-functional: every source file < 200 lines; no FFI changes.

## Architecture
```
on_incoming_files -> EngineBridge+Events -> FilesSync.receive(offer)
   isNew? (checked before offers.removeAll) -> coalescer -> onNotice -> bridge.incomingFiles.send   (phase 05 subscribes)
   auto-accept? emit then accept() : offers.insert
FilesSync.accept -> FileFetchService.fetch -> specs[id] = RetrySpec(item,file,dir,peer)
on_transfer Failed -> row.failed ; canRetry = failed && receive && spec && peerTrusted && !inFlight
UI Retry -> FilesSync.retry (guard) -> TransfersModel.remove(row) -> fetcher.fetch (new id)
devices/$phase/trust outcome/sign-out -> fetcher.pruneSpecs(...)
navigation.$section == .history -> bridge.refreshHistory()
```

## Related Code Files
- Modify: `apps/macos/FsyncMac/Package.swift`, `apps/macos/scripts/build-fsync-mac-app.sh`, `Sources/FsyncMacCore/Files/{FileFetchService,FilesSync,TransfersModel}.swift`, `Bridge/EngineBridge.swift` (two `let`s), `Bridge/EngineBridge+Clipboard.swift` (onNotice wiring, sign-out reset), `Bridge/EngineBridge+Models.swift` (devices/phase/trust sinks)
- Create: `Bridge/EngineBridge+Navigation.swift`, `Features/Navigation/MainNavigation.swift`, `Files/IncomingFilesNotice.swift`, `Files/IncomingFilesNoticeCoalescer.swift`, `Files/DropSendRequest.swift`, tests `MainNavigationTests`, `TransfersRetryTests`, `IncomingFilesNoticeTests` (notice + coalescer + sanitizer vectors), `DropSendRequestTests`, `EngineBridgeNoticeWiringTests` (bridge-level: receive → `incomingFiles` emits once; duplicate offer → no emit; auto-accept → emit before fetch)
- Delete: none
(paths under `apps/macos/FsyncMac/` unless absolute)

## Implementation Steps
1. Bump the target and the bundle plist; `swift build`.
2. `MainNavigation` + tests (optional selection, show, reset on sign-out, history refresh sink).
3. Retry specs + guards + pruning; tests: duplicate tap = one fetch; peer removed → `canRetry == false`; sign-out clears; phase change clears.
4. `IncomingFilesNotice` + coalescer + ordering in `receive`; bridge wiring test.
5. `DropSendRequest` + tests (http URL rejected, symlink resolved, duplicates removed).
6. `swift test`; fix.

## Todo List
- [x] Target bump
- [x] MainNavigation (optional section) + history-refresh sink + tests
- [x] Retry/Reveal + in-flight guard + pruning + tests
- [x] IncomingFilesNotice + coalescer + receive ordering + bridge wiring test
- [x] DropSendRequest + tests
- [x] All files < 200 lines

## Success Criteria
- `cd apps/macos/FsyncMac && swift build && swift test` green (baseline 222 run / 18 skipped + new tests, 0 failures).
- `grep -n "v14" apps/macos/FsyncMac/Package.swift` and `grep -n '"14.0"' apps/macos/scripts/build-fsync-mac-app.sh` match.
- No change outside `owns`. No `MenuBarIconState` in this phase.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Retry spec leaks memory | L x L | Bounded map, pruned on the four triggers |
| Retry to a removed/untrusted peer | M x M | `canRetry` checks the current trusted list |
| Duplicate notices | M x L | `isNew` before removeAll + coalescer + tests |
| `.v14` breaks CI | L x M | CI runner `macos-15` supports it |
| Section sink refresh loops | L x M | Refresh only on assignment of `.history`; test counts calls |

## Security Considerations
- Notice text: sanitized sender + count + size only. Drop input validated (scheme, symlinks) before any send.

## Next Steps
- Phase 03 consumes `canRetry`, `retry`, `revealURL`, `DropSendRequest`, `MainNavigation`. Rollback: revert this phase's commits (nothing depends on it yet). Revert the target bump separately if CI breaks.
