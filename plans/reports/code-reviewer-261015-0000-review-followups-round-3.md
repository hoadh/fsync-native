# Round 3 review: fix/review-followups (7b9cd0f)

Score: 9.5/10.

## Verification
- `cargo test -p fsync-core --lib stop_leak` ran 5 times in a row. All 5 runs passed 2/2, in about 0.7-0.8 s each.
- Full `cargo test -p fsync-core --no-fail-fast` is green, with no failed suites. This includes `file_transfer`, now that the disk has been freed (169 GiB available).
- `cargo build -p fsync-core` (non-test) is clean, so the `#[cfg(test)]` gating does not break the build.
- I did not run `swift test`. The Swift change is a doc comment only.

## Round-2 lows
| Item | Status |
|---|---|
| Timing-based `!is_closed` assertion | Fixed. Test 1 uses a 300 s hold, so "held and not closed" is observed with no race, then `drop(leaked)` lets the holder shut down and the lock free up. Test 2 uses a 300 ms hold and only waits up to 10 s for the lock to free. It then asserts `is_closed`, which is ordered by close-before-lock-release in the code, so there is no timing assumption. |
| `isQuitting` doc | Fixed. EngineBridge.swift:35-37 now says quit is final and sync stays off until the next boot. |
| `Node::is_closed` public | Fixed. It is now `#[cfg(test)] pub(crate)`, with no users outside the lib tests. The grep found only the test uses and an unrelated `out.is_closed()` in `api/presence.rs`. |
| Conditional hotkey `XCTSkip`, tolerant message test | Accepted, as in round 2. |

## Regression check
- None found.
- Test 1 leaves a spawned holder polling every 250 ms with a 300 s max hold, but only until `drop(leaked)`. The holder then finishes, so there is no leaked task or lock past the test.
- Test 1's `svc.start()` after the lock is freed also proves the Arc unwrap, shutdown and restart cycle works.
- Test 2's `drop(leaked)` after close, plus a restart, still covers close idempotency.

## Remaining
- Info: the two Postgres container-start failures at `backend/tests/common/pg.rs:47` are outside this branch's diff. Docker contention is the plausible cause. It is worth confirming the backend tests are green when run alone, but it is not a blocker for this branch.
- Info: test 2's 10 s wait is only a failure bound. On a heavily loaded CI box a 300 ms hold plus close should still complete well inside it.

No critical, high, medium or low issues remain.

**Status:** DONE
**Summary:** All round-2 lows are resolved. The leak tests are deterministic over 5 consecutive runs, `is_closed` is test-only, the doc is fixed, and the full fsync-core suite passes with no regressions.
**Concerns/Blockers:** None for this branch. The Postgres harness flake at `backend/tests/common/pg.rs:47` is outside the diff. `swift test` was not run (the only Swift change is a comment).
