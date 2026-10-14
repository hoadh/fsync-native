# M5 trust-reset confirm API: finalize sync-back

Date: 2026-10-14. Tree: uncommitted M5 change. Host: macOS arm64.

## Gate evidence (tester 11:48)
fmt PASS · clippy `-D warnings` PASS · workspace tests 253 pass / 0 fail / 1 ignored (by design) · xcframework PASS · Swift 8/8 · e2e PASS (1 run).

## Synced
| File | Change |
|------|--------|
| plans/261013-2029-fsync-native-mvp/plan.md | Gate-green line; e2e and Mac-Windows open; M5 moved from deferred to done; app UI and email notice listed as not built |
| phase-06 | M5 "Deferred" line replaced with "Done". No todo ticked. Status stays Partial |
| phase-08 | M5 "Deferred" line replaced with "Done" (FFI and CLI). App prompt UI noted as not done. No todo ticked. Status stays Partial |
| phase-09 | Trust-reset backend coverage and email notice (not built) noted. No todo ticked. Status stays Partial |
| docs/development-roadmap.md | Open item 1 closed for the gate (flake root cause still open). Item 2 now = 2 more e2e runs |
| docs/project-changelog.md | Verification status rewritten with final-tree results; interim run marked superseded; round-3 entry points to it; known issues add app UI and email notice |

## Not ticked (on purpose)
- macOS app prompt UI for the reset event (phase 11/12, not started).
- Email notice on reset (not built, not in ADR 012).
- Phase 08 "P0 exit demo" (Mac-to-Windows not run) and "FFI adapters + CI artifacts" (no CI publish job). Both remain open.
- DeviceList "100% branch" coverage (phase 06) not measured.

## Phase and plan status
Complete 0/16. Partial 01, 02, 03, 06, 07, 08, 09 (unchanged). Scoped todos 28/37 (unchanged).

## Next actions
1. Main agent: review and commit the M5 change (conventional commit, no AI references). Owner: main agent. Done when: `git status` is clean and the commit is on the working branch.
2. Main agent: run the e2e demo 2 more times and record results in changelog Verification status. Owner: main agent. Done when: 3 PASS runs on the committed tree (relay-only round included).
3. Main agent: decide on the `fsync_ffi.swift` regeneration diff check (tester rec 1), and whether the 1 GiB bench is a release gate. Owner: main agent. Done when: decision noted in phase-15 or the roadmap.
4. Owner C (macOS): build the reset prompt UI on `pending_trust_resets` and the `TrustReset` event (phase 11/12). Done when: the prompt shows device names, and confirm and decline call the FFI.
5. Main agent: finish the plan. The P0 exit gate still needs the Mac-to-Windows run and the phase-08 CI artifacts. Important: finish the plan, not just the M5 slice.

## Concerns
- Roadmap rows 06, 07, 08 and 09 say "Done", but their phase files say Partial with open todos. Left as-is (light touch). Please align.
- The e2e target is 3 runs. Only 1 is recorded for the final tree.
- The `devices_reset` PortNotExposed failure has no root cause and no fix (`backend/tests/common/pg.rs`).

## Unresolved questions
- Was `fsync_ffi.swift` byte-identical after the xcframework regeneration? Not verified (no pre-run copy).
- Should roadmap rows 06-09 be moved back to "Partial" to match the phase files?
- Should the 1 GiB benchmark (`--ignored`) be a release gate?
- Pending and declined trust-reset state is in memory only. Persist it, or accept the re-prompt on restart?
- Bump the FFI contract version again? It is 2 now, and the M5 round-2 change is breaking for Swift `switch` statements.

**Status:** DONE_WITH_CONCERNS
**Summary:** Synced plan.md, phases 06, 08, 09, roadmap and changelog to the final-tree green gate and the M5 API. No phase todo ticked, since none maps to M5 as completed work. P0 exit is still open.
**Concerns:** Roadmap status rows disagree with phase files. e2e has 1 of 3 runs. devices_reset flake root cause still unknown.
