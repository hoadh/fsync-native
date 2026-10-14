# Sync-back: phase 11 macOS app shell (branch macos/phase-11)

## Progress
- Phase 11 (macOS app shell): Done on Mac. Manual visual checks open.
- Plan `261014-1235-macos-app-shell-pairing`: status completed. Phases 1-6 completed. Todos ticked: all automatable ones.
- Gates met: review 9.6 (0 critical), FsyncMac 89 tests (3 integration skipped by default), FsyncCoreKit 8+14, integration PASS, Cargo 253 passed / 1 ignored.
- Phase 11 spec todos: 7/7 ticked. Status set to "Done on Mac (manual visual checks open)".
- MVP phase 11 row updated. Phase 12 note added in MVP progress section.

## Todos (checked against done criteria)
- Ticked: all phase 1-3 todos (already ticked), phase 4 and 5 automated todos (already ticked), phase 6 code review (9.6, 0 critical).
- Unticked, manual, open:
  - Phase 4: remove and trust-reset confirmationDialogs.
  - Phase 5: sign-in from bundle, launch-at-login toggle, Settings window visual.
  - Phase 6: manual checklist report (covers launch window, MenuBarExtra focus, dialogs, launch at login).
  - Keychain prompts and launch-window close: not tied to a todo, listed in plan Open items.

## Blockers (stalled, owner, unblock)
- Manual visual checks (stalled since 2026-10-14, all sessions). Owner: user, needs a person at a Mac with the bundle from `build-fsync-mac-app.sh`. Unblock: run the checklist, record results in a report, then tick the phase 4-6 todos.
- Signed-build Keychain migration: not run. Owner: user (signing decision). Unblock: Developer ID decision, then one signed build run.

## Scope changes
- SwiftPM package `apps/macos/FsyncMac/` instead of xcodeproj. Reason: no Xcode project needed for CLI and script builds. Impact: none on delivery.
- Status shows "Online" until phase 12 connects peers. Reason: core reports Lan/Relay only for live QUIC. Impact: phase 12 must verify Lan/Relay.
- Keychain falls back to file keychain (ad-hoc gives -34018). Impact: migration untested until signed build.
- Added: "Lost your other devices?" recovery (validation S1). Impact: small scope add, covered by tests.
- Removed: "Recent items" popover placeholder. Impact: none.

## Risks
- New: Keychain migration could create a new node identity if a signed build finds no data-protection item (review Medium 1 path). Mitigation: verify on signed build in phase 12.
- New: Lan/Relay status unverified in the app until sync. Mitigation: phase 12 owns it.
- Open: core offline `logout()` leaves refresh token valid server-side. Needs a core change, owner core (phase A).
- Open: integration script not in CI. Owner: phase 10 (B).
- Open: launch window close timing on macOS 13-15. Manual.
- Closed: none this sync.

## Next actions (main agent must finish these)
Finishing the plan is important. The phase 11 sign-off stays open until these are done.
1. Main agent: run the manual checklist from the bundle, or hand it to the user. Done when the four manual items (plus Keychain prompts and launch window) have recorded results and the phase 4-6 todos are ticked.
2. Main agent: decide the Developer ID / signing question. Done when the decision is written in the MVP plan (unresolved Q4) and the signed-build Keychain check is scheduled for phase 12.
3. Core owner: fix offline `logout()` refresh-token revocation or record it as accepted risk. Done when the decision is in the changelog.
4. Phase 10 owner: wire `run-fsync-mac-integration.sh` into CI or record the reason it stays manual. Done when CI status is recorded.
5. Main agent: optional round-3 Low items (LoginModel waits for pending clear; boot awaits `sessionClear`). Done when fixed with tests, or recorded as accepted.

## Files changed
- `plans/261014-1235-macos-app-shell-pairing/plan.md`: status, phase 6 row, results, open items.
- `plans/261014-1235-macos-app-shell-pairing/phase-06-integration-test-and-docs.md`: overview status, code review todo.
- `plans/261013-2029-fsync-native-mvp/phase-11-macos-app-shell-pairing.md`: status, plan link, todos, deviations.
- `plans/261013-2029-fsync-native-mvp/plan.md`: progress, phase 11 row, phase 12 note, unresolved Q4-7.
- No code edited. Phase 12 file not edited (note placed in MVP plan per request).

## Unresolved questions
1. Is a Developer ID or provisioning profile planned? It decides when the Keychain migration is verified.
2. Does the backend rotate refresh tokens? It sets how much the core offline-logout gap matters.
3. Does the launch window close reliably on macOS 13, 14 and 15?

**Status:** DONE_WITH_CONCERNS
**Summary:** Phase 11 synced. Plan completed, phase 11 done on Mac with manual visual checks open. Ticked all automatable todos, recorded results, deviations and carry-over items. Plan is not fully finished until the manual checks and the signing decision are done.
**Concerns/Blockers:** Manual visual checks and the signed-build Keychain check are stalled waiting on a person at a Mac and the signing decision. Core offline-logout gap and CI gap are open with owners in next actions.
