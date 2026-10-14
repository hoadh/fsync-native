# macOS trust-reset prompt UI (standalone feature)

Status: Completed · Priority: P1 · Owner: macOS
Context: ADR 012, docs/ffi-contract.md (v2 trust-reset API), phase-11 (app shell, not built yet).
User decision: build as standalone SwiftUI feature in a library next to FsyncCoreKit; phase 11 plugs it in.

## Scope
- In: prompt shown when another device resets account trust (`AccountEvent.trustReset` / `pendingTrustResets()`), confirm / decline, outcome + error states.
- Out (YAGNI): initiating reset UI (`resetAccountTrust`), app shell, delegate adapter, persistence of declines.

## Design
- New library target `TrustResetPrompt` in `apps/macos/Packages/FsyncCoreKit/Package.swift` (depends on FsyncCoreKit), macOS 13.
- `TrustResetPromptModel` (@MainActor ObservableObject) over `FsyncEngineProtocol`:
  - `confirm(resetId:)` / `decline(resetId:)` take the id the user saw (consent binding); the model never substitutes the current one. The view captures the id when the confirm dialog opens and closes it when the shown id changes ("The device list changed. Review it again.").
  - `refresh()` ← `pendingTrustResets()`; `handle(_ event: AccountEvent)` refreshes on `.trustReset`.
  - `confirm(resetId:)` / `decline(resetId:)` run engine calls off main; map `AccountState` → outcome (`.ready` trusted, `.needsPairing` this Mac removed → pair again).
  - `FsyncError.NotFound` → refresh + "request changed or was answered, review the latest".
  - Other errors → readable message; busy flag blocks double taps.
- `TrustResetPromptView`: warning header, plain explanation ("only confirm if you or someone you trust did this"), device names rendered verbatim (untrusted text), no reset-id code (the resetting device cannot show one: no API exposes the head hash and the id changes if the chain grows), so the user is told to check the device names; neutral bullets (platforms unknown); "request changed" notice in orange, real errors red; buttons: "Keep current devices" (default) / "Trust new devices" (explicit second confirm when destructive).
- Files < 200 lines each.

## Testing (no mocks)
- Model against a real `FsyncEngine` (fresh temp dir): empty pending, NotFound on stale id → message + refresh, event handling with real `AccountEvent` values.
- `ImageRenderer` render test of the view (pending + outcome states) → PNG for visual check.
- Happy confirm/decline end to end is covered by Rust real-backend tests (service_trust_reset*); Swift has no backend harness.

## Phases / todo
- [x] Target + model + view
- [x] Swift tests + render snapshots
- [x] Code review ≥9.5, swift test + cargo gates green
- [x] Docs (ffi-contract app guidance, changelog, codebase-summary); phase-11 note added (phase-12 not relevant)

## Results
- Review rounds 7.0 → 9.2 → 9.6 (auto-approved, 0 critical).
- Gates: swift 14 + 8 strict concurrency, 0 warnings; cargo fmt clean; cargo workspace 253 passed / 1 ignored.

## Open
- confirmationDialog not visually verified (ImageRenderer cannot render dialogs). Check in phase 11.
- Runnable demo app not built; snapshot tests used instead.
- Low findings L-f, L-g, L-h (see code review round 3).
- Phase 11 must wire EngineDelegateAdapter → `model.handle(event)` on the main actor and route `.removedNeedsPairing` to pairing.
