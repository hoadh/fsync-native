# Code re-review: phase 01 fix round (bb50a66, 908719e, 344ed35)

Score **9.0/10** (was 7.5). Medium 1, low 4. NOT approved (bar 9.5). Persisted by the controller from the reviewer's hand-back. Builds ran on archive copies on disk.

## Verified
- fmt and clippy clean (lib in production features; core + cli all-targets). Tests: 243 passed, 0 failed, file_transfer skipped (disk quota).
- **H1:** the pre-fix `responder.rs` mutant fails all 3 new tests in 10 of 10 runs, each on the intended assert.
- **M3:** with the guard's Drop turned into a no-op, `shutdown_during_the_drain_still_reports_completed` fails 12/12. Drop order: `Completed` is emitted, then `windows.end(Ok)`.
- **M1:** docs reworded; the cap test asserts 4.5-6.5 s.
- **L1-L7** and **M2** done as described.

## Medium
- **M-1:** `pair-flake-loop.sh:27,40`: an all-pass run calls `rm -rf "$OUT"` on a caller-supplied `PAIR_LOOP_OUT` (for example `~/logs`), which deletes that whole directory. Fix: delete only a directory the script created itself, or refuse an existing non-empty dir.

## Low
- **L-a:** `responder.rs`: `ticket.set_pinned` runs before `Done` is sent, but the guard's pinned value is set only on `Ok`. Scenario: sending `Done` fails, then a shutdown interrupts the drain. The window reports `Ok(seq)`, but no `Completed` or `Failed` event fires. Fix: set both flags at the same point.
- **L-b:** the `InitiatorHooks` branches (including the 10 s `keep_open` spawn) are compiled into production. They are unreachable there; still, gate them with `cfg(feature = "test-hooks")` or `#[doc(hidden)]`.
- **L-c:** ADR 014 claims the round-1 test failed pre-fix every time through its outcome assert. The reviewer saw it pass 16 of 16 pre-fix. Reword to "timing-dependent".
- **L-d:** shellcheck not run (not installed).

## Unresolved questions
1. Should M-1 be fixed before merge? The reviewer says yes.
2. L-a now?
3. Merge before X1+X2+X5?
