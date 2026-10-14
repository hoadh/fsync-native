# Sync-back: macOS trust-reset prompt UI

## Plan status
- `plans/261014-1206-macos-trust-reset-prompt/plan.md`: Completed.
- Todos: all checked, including review/gates (was line 30).
- Results recorded: review 7.0 → 9.2 → 9.6 (auto-approved, 0 critical). Swift 14 + 8 strict concurrency, 0 warnings. cargo fmt clean. Cargo workspace 253 passed / 1 ignored.

## Phase updates (MVP plan)
- `phase-11-macos-app-shell-pairing.md`: added note. Library exists. Wire `EngineDelegateAdapter` → `model.handle(event)` on main actor. Route `.removedNeedsPairing` to pairing.
- `phase-12-macos-clipboard-files-history.md`: not updated. Trust-reset is account-level, no clipboard relevance.

## Open items
- `confirmationDialog` not visually verified (ImageRenderer cannot render dialogs). Owner: phase 11.
- Runnable demo app not built. Snapshot tests used instead.
- Low findings L-f (no stub-engine test for non-NotFound branch and id passthrough), L-g (dialog count uses `info.devices`, list uses `deviceNames`), L-h (silent drop of Network/Auth error for non-first reset). Owner: phase 11 or follow-up.
- Phase 11 must wire the adapter and route `.removedNeedsPairing`. Not started (phase 11 todos unchecked).

## Concerns
- Phase 11 is still pending. Trust-reset UI is only usable once it is wired into the app shell.
- Hook-injected paths (`apps/macos/Packages/FsyncCoreKit/plans/...`) differ from the task paths. Used the task paths (`fsync-native/plans/`) as instructed.

## Unresolved questions
- Should L-g be fixed now (`deviceNames.count`) rather than deferred? It is a one-line change but the task said no code edits.
- Should phase 12 get a note anyway, in case the clipboard watcher also consumes engine events?

**Status:** DONE
**Summary:** Plan marked completed with results and open items. Phase 11 note added. Phase 12 skipped as not relevant. No code edited.
**Concerns/Blockers:** Phase 11 (app shell wiring) is still pending. It is the next step that must finish the trust-reset integration.
