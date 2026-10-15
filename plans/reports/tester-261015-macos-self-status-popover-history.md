# Tester Report: macOS Swift package (self-status popover, history refresh)

Date: 2026-10-15. Pkg: apps/macos/FsyncMac. No source edits made.

## Results
- Build (`swift build`): PASS, no warnings or errors.
- Test (`swift test`): PASS, exit 0.
- Total: 234 cases = 226 Core + 8 UI.
  - Core (FsyncMacCoreTests.xctest): 226 executed, 208 passed, 18 skipped, 0 failed.
  - UI (FsyncMacUITests.xctest): 8 executed, 8 passed, 0 failed.
- Failures: none.

## Baseline delta
- Baseline 222 = Core only. Correct comparison: 227 (222 Core + 5 UI OnboardingRenderTests).
- Now 234 = 227 + 7 new. Matches the 7 new tests.
- New tests, all passed:
  - EngineBridgeHistoryRefreshTests (Core): 4
  - MenuWindowTopAnchorTests (UI): 3
- Expected "229" missed the UI bundle. Not a regression.

## Skipped (18, all Core, pre-existing)
- ClipboardIntegrationTests: 7
- FilesIntegrationTests: 3
- FsyncMacIntegrationTests: 3
- RealBackendRevokeTests: 3
- PendingRevokeIntegrationTests: 2
- Integration suites; likely env/backend-gated. Count unchanged from baseline.

## Warnings
- One warning, not in changed files:
  - `Tests/FsyncMacUITests/OnboardingRenderTests.swift:25` - FocusState accessed outside View body (testSettingsRender).
- No warnings in changed files:
  - FsyncMacApp.swift, EngineBridge+Clipboard.swift, EngineBridge+Events.swift, HistoryModel.swift, FilesSync.swift, DeviceRowView.swift, MenuPopover.swift, MenuWindowTopAnchor.swift.
- Caveat: incremental build. Warnings from unchanged-since-last-compile files are not re-emitted.

## Diff stat (apps/macos)
- 7 files changed, 36 insertions(+), 7 deletions(-).
- Untracked new: MenuWindowTopAnchor.swift, EngineBridgeHistoryRefreshTests.swift, MenuWindowTopAnchorTests.swift.

## Concerns
- Pre-existing SwiftUI FocusState warning in OnboardingRenderTests (not from this change).
- Skips (18) are pre-existing; not investigated for validity.

## Unresolved
- Are the 18 integration skips gated on a live backend or env? Not verified.
- Test run was not repeated for flakiness; ran twice, both green, same counts.
