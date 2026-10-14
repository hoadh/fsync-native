# Code review — macOS TrustResetPrompt (round 1)

**Score 7.0/10 · Critical 1 · High 1 · Medium 3 · Low 6** → fix loop. swift test (strict-concurrency=complete) green, no new warnings.

## Critical
- C1 confirm()/decline() read `pending.first?.resetId` at call time, not the id shown when the user tapped; `confirmingTrust` dialog not reset when id changes → server can grow list during dialog and the confirm pins list B the user never saw (breaks ADR 012 consent binding). Fix: `confirm(resetId:)`/`decline(resetId:)` from view's `info.resetId`; reset dialog on id change.

## High
- H1 no regression test for id binding (swap pending A→B during confirm, engine must get A → NotFound).

## Medium
- M1 errorMessage/errorIsNotice outlive the prompt (not cleared on refresh/new id/dismiss).
- M2 device list unbounded, not scrollable; tail truncation hides suffixes. (Core `check_shape` already rejects control/bidi chars and names >64.)
- M3 render tests weak; AccountState→outcome mapping untested; Auth/ReauthRequired/default messages untested.

## Low
- L1 notice set when nothing pending is invisible in empty view. L2 `TrustResetOutcome.none` ambiguous. L3 phase 11 must route `.removedNeedsPairing` to pairing. L4 orange contrast on light bg. L5 decline closure (moot after C1). L6 Package.swift target ordering.

## Unresolved
- Dialog should list device names? (lead: yes) · Visible "list changed" notice while dialog open?

---

## Round 2 — 9.2/10 (0 Critical, 0 High, 2 Medium, 5 Low) — below 9.5 gate

- **M-A** Dialog destructive button read `@State confirmingId` at tap time; binding setter nils it, so if SwiftUI dismisses first the answer is dropped (fails safe). Fix: `confirmationDialog(..., presenting:)`.
- **M-B** `testAnswerUsesTheIdPassedNotTheCurrentOne` cannot detect a C1 regression; name/comment overclaim.
- **L-a** Model.swift:105 — non-NotFound error from attempt on A overwrites the "list changed" notice shown for B.
- **L-b** Dialog lists 6 names + "and N more"; show the count prominently.
- **L-c** 6 rows × 2 lines unscrolled can grow the window (tolerable).
- **L-d** RenderTests:57-60 indentation.
- **L-e** `onAppear` calls `refresh()` synchronously (fine today).
- plan.md:16 still says `confirm()`/`decline()`.

Fixes applied after round 2: M-A (`presenting: confirmingId`, id captured into the action), M-B (renamed `testListChangeDuringAnswerRefreshesAndTrustsNothing`, honest comment), L-a (`finish(id:)` shows non-NotFound errors only while the answered reset is still shown), L-b (message says "these N devices"), L-d, plan.md:16. L-c/L-e accepted.
Unresolved: macOS `confirmationDialog` message rendering not visually verified (ImageRenderer cannot render dialogs).

---

## Round 3 — 9.6/10 (0 Critical, 0 High, 0 Medium, 3 Low) — passes gate (>=9.5, 0 Critical)

Gates re-run: `swift build` ok; `swift test -Xswiftc -strict-concurrency=complete` 14 + 8 pass, no warnings.

### Round-2 fixes verified
- **M-A** PendingView.swift:80-93: `presenting: confirmingId`; `onTrust(id)` uses the closure value, so the binding setter nilling `confirmingId` cannot drop the answer. `.id(info.resetId)` (PromptView.swift:70) plus `dialogIsPresented` still close the dialog on id change. OK.
- **M-B** ModelTests:59-76: renamed, comment says it does not prove the sent id. Honest. OK.
- **L-a** PromptModel.swift:97-113: NotFound refreshes then shows the notice; other errors show only if `pending.first?.resetId == id`. Correct; the "list changed" notice is no longer overwritten. OK.
- **L-b** PendingView.swift:101-102 "these N devices". OK. **L-d** and plan.md:16 (`confirm(resetId:)`/`decline(resetId:)`) OK.

### Low (non-blocking)
- **L-f** PromptModel.swift:107 — the non-NotFound branch of `finish` (shown only while the id matches, suppressed otherwise) has no test. The real-engine harness cannot produce Network/Auth for confirm. A small recording stub of `FsyncEngineProtocol` (about 20 methods) would cover this and also the id passthrough (the real C1 guard, which is currently held only by the signature). Suggest for phase 11 or a follow-up.
- **L-g** PendingView.swift:101 — the dialog count uses `info.devices` while the list uses `deviceNames`. If the core ever lets them differ, "these 5 devices" could sit above 3 names. Use `deviceNames.count` or confirm that core guarantees equality.
- **L-h** PromptModel.swift:107 — a Network/Auth error for a reset that is no longer first (for example the list just emptied) is silently dropped. This is acceptable because the view has moved on; note it in the phase 11 integration notes.

Carried over, accepted: L-c, L-e.

### Unresolved
- The `confirmationDialog` message on macOS was not visually checked (ImageRenderer cannot render dialogs). Do a manual check in the app integration phase (6 names plus "and N more", long names).
- Phase 11 must route `.removedNeedsPairing` to pairing (R1 L3).
