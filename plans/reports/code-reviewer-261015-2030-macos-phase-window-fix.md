# Review: macOS phase/window willSet race fix
Score: 8/10

## Correctness
Fix is correct for the repro. `@Published` emits in willSet; async hop runs after storage set, so the new RootView reads the new phase. autoOpened set synchronously, so no double-open. .ready/.booting paths unchanged.

## Other paths, same race?
- windowRequest (Events.swift:32): PassthroughSubject, not willSet. trustModel.handle runs before send(). Safe. Window already open = subscribed to trustModel; closed = fresh render reads current state.
- MenuPopover openMain: user-driven (button), phase already stored. Safe.
- RootView: observes bridge via ObservedObject; only missed the change due to the early window creation. No change needed.
- EngineBridge+Clipboard `$phase.sink`: reads `phase` param, not self.phase. Safe (uses value). Models.swift reads self.phase elsewhere, not in a sink.

## Issues
Critical: none.
Major: none.
Minor:
1. Stale phase: async closure opens window even if phase moved on before the next turn (e.g. needsPairing -> ready fast). Re-check `bridge.phase` inside the async block (skip if .ready/.booting).
2. Root cause is a generic footgun: any future `$phase` subscriber calling into UI synchronously repeats this. Prefer `bridge.$phase.receive(on: DispatchQueue.main)` (or `.dropFirst`/removeDuplicates) at the subscription so the guard is in one place; then the inline async is unnecessary.
3. showMain's nested DispatchQueue.main.async for makeKeyAndOrderFront now runs one turn after openWindow in a chain of two hops; window may not exist yet on slow creation. Low risk, same as before.
4. Comment fine; consider noting it applies to all `$published` sinks.

## Test feasibility
App-target view code: not unit-testable cheaply. Feasible in FsyncMacCore: test that a `$phase` sink observes the OLD `bridge.phase` synchronously (documents willSet), and with `.receive(on: .main)` observes new one after a runloop turn. Needs a bridge test double; mid value. Manual verification already done.

## Unresolved
- Is the history window / Settings affected? Not auto-opened, so no.

**Status:** DONE
**Summary:** Fix correct, no other same-race paths found; two minor hardening suggestions.
**Concerns:** Stale-phase re-check (minor); consider receive(on:) at subscription.

---

# Round 2
Score: 9/10

## Verification of round-1 minors
1. Stale-phase re-check: done. `DispatchQueue.main.async { if bridge.phase != .ready { showMain() } }` reads stored value post-willSet. Correct. Residual: a `.booting` value at that moment would still open window; unreachable in practice (booting is initial state only). Not worth guarding (YAGNI).
2. `receive(on: .main)` revert: accepted. Rationale sound: the `.onReceive` sits in a `MenuBarExtra` label, `bridge.$phase` yields a fresh publisher per body eval, so resubscribe drops the in-flight scheduled delivery. Explicit hop inside the closure is independent of subscription lifetime, so more robust here. Round-1 suggestion withdrawn. Suggest the comment mention this (why not `receive(on:)`) so nobody re-"fixes" it.
3. Double deferral in showMain: empirically verified (NSLog; unsigned `.bootFailed` render + signed pairing screen). Withdrawn.

## Diff check
- `autoOpened = true` set synchronously before hop: no double open from rapid phase emissions. Good.
- Closure captures `bridge` (class, StateObject) and `self` (struct App, value copy); `@State autoOpened` not touched in the async block. No retain/cycle concern.
- Main-thread: `$phase` emits on main (UI-bound model) and the hop is main to main; no threading change.
- Edge: if phase goes needsPairing -> ready before the hop, no window, and autoOpened stays true. Intended.

## Issues
Critical: none.
Major: none.
Minor:
1. Comment should note why `receive(on:)` is not used (per-evaluation publisher resubscribe) to prevent regression.
2. No automated coverage (app target has no tests; 222 run / 18 skipped / 0 fail is from prior iteration, not re-run on this diff, and does not exercise this file). Manual verification only. Acceptable for a 4-line view-glue change.

## Unresolved
- Not re-run: Swift tests / build on the final diff. Confirm `swift build` passes (Equatable on phase assumed, already used by `bridge.phase == .ready` at line 42, so low risk).

**Status:** DONE
**Summary:** Fix correct and minimal; round-1 minors resolved or validly withdrawn. No critical/major.
**Concerns:** Add a comment on why not `receive(on:)`; tests not re-run on final diff.
