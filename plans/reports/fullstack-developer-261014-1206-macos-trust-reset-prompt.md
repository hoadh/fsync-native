# Report: macOS trust-reset prompt (after code review round 1, 7.0 -> fixes)

## Files (apps/macos/Packages/FsyncCoreKit)
- Sources/TrustResetPrompt: TrustResetPromptModel (119), TrustResetPromptView (99, wrapper + stateless content), TrustResetPendingView (112, question + dialog state), TrustResetOutcomeView (47), TrustResetMapping (39, pure mappings)
- Tests: TrustResetPromptModelTests (7), TrustResetMappingTests (3), TrustResetPromptRenderTests (4)
- Package.swift targets grouped with comments; docs/plan/changelog updated

## Review findings
- C1 fixed: `confirm(resetId:)`/`decline(resetId:)`; view captures id when dialog opens (`confirmingId`), dialog presented only while it equals the displayed id (`dialogIsPresented`), pending view is `.id(resetId)` so state resets on change. Dialog message lists device names (first 6 + "and N more").
- H1: test swaps `pending` A->B after tapping with A; asserts unanswered outcome, notice, refresh from engine. Limit: engine knows neither id so A and B both give NotFound; test proves no trust happens and the id path, not which id reached the engine (no mocks; a forwarding engine for a 40-method protocol was not worth it). The id-binding itself is by construction (id is a parameter; model has no lookup).
- M1: `apply(_:)` clears message when first id changes and sets "The device list changed. Review it again." when one id replaced another; `dismissOutcome` and answer start clear it. Tested.
- M2: >6 names -> ScrollView max 150pt; names wrap to 2 lines; comment cites core `check_shape`. ImageRenderer draws no ScrollView content, so that case is measured (height capped) but not saved as a PNG.
- M3: `TrustResetMapping.outcome/message` pure, tests cover Ready/Created/NeedsPairing and NotFound/Network/Auth/ReauthRequired/default. Render tests now compare dark vs light luminance, message adds height, outcome shorter than pending, list height capped; still smoke tests (no controls driven).
- L1: no notice when nothing pending (no guard at all; id goes to engine). L2: `.unanswered`. L3: plan Next section. L4: `noticeOrange` dynamic (dark orange in light mode; checked pending-notice.png). L6: Package.swift grouped.

## Results
- `swift build` ok; `swift test -Xswiftc -strict-concurrency=complete`: 14 TrustResetPromptTests + 8 FsyncCoreKitTests, 0 failures, no new warnings; plain `swift test` also green (8 FsyncCoreKitTests final line).
- Snapshots cleared and regenerated (10 PNGs) in /private/tmp/claude-501/-Users-alex-Code-fsync-native/8b5775ea-33f8-41e0-b343-5bcd5c68080c/scratchpad/trust-reset-snapshots.
- Rust untouched. Happy confirm/decline covered by core/fsync-core/tests/service_trust_reset*.rs.

**Status:** DONE_WITH_CONCERNS
**Summary:** C1, H1, M1-M3, L1-L4, L6 addressed; build and all Swift tests pass under strict concurrency.
**Concerns:** H1 test cannot distinguish A from B at the engine (both unknown); confirmation dialog contents not visually verified (ImageRenderer cannot show it); scrolling list not snapshotted.
