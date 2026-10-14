# macOS Trust-Reset Prompt: Consent Binding Caught in Review

**Date**: 2026-10-14 12:25
**Severity**: Medium
**Component**: apps/macos/Packages/FsyncCoreKit (TrustResetPrompt), consumes M5 confirm API
**Status**: Resolved for library scope. Demo app and phase 11 wiring still open.

## What Happened

Built `TrustResetPrompt`, a standalone SwiftUI library (model plus views) next to FsyncCoreKit. It presents the pending trust reset, confirms or declines it, and shows the outcome. Tests run with `swift test` against the real engine, not a stub. When `TRUST_RESET_SNAPSHOT_DIR` is set, ImageRenderer writes PNG snapshots. Review went 7.0, then 9.2, then 9.6 (gate is 9.5 with zero Critical).

## The Brutal Truth

Round 1 had a Critical bug that would have broken the whole point of the prompt. `confirm()` and `decline()` read `pending.first?.resetId` at call time. The dialog could be showing reset A while the server swapped in reset B, and the user would confirm B without ever seeing it. I wrote that code. It passed the tests because the tests never swapped the list mid-dialog.

## Technical Details

- Fix: `confirm(resetId:)` and `decline(resetId:)` take the displayed id. The model has no lookup.
- `confirmationDialog(..., presenting: confirmingId)` captures the id when the dialog opens. Round 2 found that the binding setter could nil the state before the button fired. Fixed.
- `.id(info.resetId)` on the pending view resets local state on id change.
- Notice logic: NotFound triggers a refresh and shows an orange notice. Other errors show only while the displayed id still matches, so a stale attempt cannot overwrite the "list changed" notice.
- `@Combine.Published`, because FsyncCoreKit exports its own `Published` struct.
- Device names render with `Text(verbatim:)`. They are untrusted.
- Gates: `swift test -Xswiftc -strict-concurrency=complete` 14 + 8 green, 0 warnings. `cargo test --workspace` 253 passed, 1 ignored (1 GiB bench).

## What We Tried

- Round 2 test `testAnswerUsesTheIdPassedNotTheCurrentOne` could not detect a C1 regression. Renamed and the comment was made honest.
- The engine returns NotFound for both A and B in the test, so the test proves no trust happens. It does not prove which id was sent.

## Root Cause Analysis

Consent state was read at action time instead of carried from the moment of display. The UI treated "the current pending item" as if it were "the item the user saw." Nothing in the type system stopped that.

## Lessons Learned

- Pass the displayed identity into every consent action. Never look it up again.
- A test that cannot fail for the bug is not a regression test. Check it against the bug.
- Snapshots cannot show dialogs. Do not count PNGs as visual verification.

## Next Steps

- Visually verify the confirmation dialog with 6 names, "and N more", and long names. Owner: whoever does the demo app. No date.
- Build the demo app. Not started.
- Phase 11: `EngineDelegateAdapter` must hop to the main actor before calling the handle. `.removedNeedsPairing` must route to pairing (R1 L3). Owner: phase 11 owner. No date.
- Low, open: the non-NotFound branch of `finish` has no test. A recording stub of `FsyncEngineProtocol` would cover it. Owner: follow-up.
- Low, open: the dialog count uses `info.devices` but the list uses `deviceNames`. Use `deviceNames.count` or confirm core guarantees equality.

**Status:** DONE_WITH_CONCERNS
**Summary:** The prompt library is built and reviewed to 9.6 with the Critical consent-binding bug fixed. Swift 14+8 and cargo 253/1 are green.
**Concerns/Blockers:** The dialog has not been visually verified. The demo app is not built. Phase 11 wiring is not done. Two Low findings are open.
