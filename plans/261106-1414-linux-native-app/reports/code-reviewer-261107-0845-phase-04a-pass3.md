# Review pass 3: phase 04a, fix round 2

Score: **9 / 10** (pass 2: 8.5). Critical: 0. High: 0. Medium: 0. Low: 2 open new issues, plus the earlier accepted lows.

## Verification (run by the reviewer)
- `cargo test --workspace` (apps/linux): all green. Platform lib 120, app 37, integration suites 2/2/3/5/7/7/8/9.
- clippy `--workspace --all-targets`: 0 warnings/errors, both with default features and with `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`.

## Earlier findings
| ID | Status | Evidence |
|---|---|---|
| L-N1 SessionExpired leftovers | FIXED | `ops_account.rs:66` calls `inbox_runtime::reset_account_files`, the same helper as logout (`ops_logout.rs:24`) and SelfRemoved (`ops_account.rs:51`). It is tested in `offers_tests.rs`. |
| L-N2 History load brings rows back | MOSTLY FIXED | The helper invalidates `OpKind::History`. A small window remains: see P3-1. |
| L-N3 refusal text | FIXED | `history_ops.rs:187-191` passes the real `FetchError` text (invalid id vs "Already downloading."). |
| L-N4 retention keep snapshot | FIXED | `sweep` takes a `claim` closure. `retention.rs remove()` holds the item's `DownloadGuard` for the whole `remove_dir_all`, and a running or newly started download makes the claim fail, so the item is skipped. Test: `an_item_whose_download_starts_mid_sweep_is_skipped`. |
| L-N5 explicit downloads have no size cap | DOCUMENTED | Phase 04b Key Insights and the ADR 014 note. |
| L1 file sizes | PARTIAL (accepted) | inbox.rs 258, retention.rs 279, inbox_tests.rs 313 are over the 200 target, all under 500. |
| L6 / L7 / L8 | OPEN (accepted, as before) | |

## Regression scan (focus areas)
- **Claim release on every path:**
  - In retention, `_claim` is a local binding that drops when `remove()` returns, on success, on an error, and on unwinding.
  - In History, e2e and eager downloads, the guard lifetime is unchanged from pass 2 (RAII, dropped when the thread ends or on panic).
  - No guard is stored in a long-lived structure.
- **Deadlocks between the claim set and retention:** none.
  - `begin_download` only locks the set for the `insert` (temporary guard, dropped at the end of the statement). `DownloadGuard::drop` locks it only for the `remove`.
  - Retention holds no other lock while it claims or removes. `scan` and `remove_dir_all` run without the set locked.
  - No code path holds the set lock while taking another mutex (`offers`, `history`, `large`) or the other way round.
- **Side effect of claim-before-remove:** for the few milliseconds of a `remove_dir_all`, a download of the same item is refused with "Already downloading." (eager lane: "Could not receive the files: Already downloading."). This needs an old item to be received again at the same moment, so it is practically never seen. Acceptable.
- **Model pokes on the right thread:** `reset_account_files` runs on the GUI thread (logout invokable), the core callback thread (SelfRemoved, SessionExpired) and test threads. In all of them it only takes short `FilesState` locks, bumps the gate and calls `hub.poke_*`, which go through `CxxQtThread::queue`. Nothing touches Qt objects directly and nothing blocks the callback thread. `FilesState::reset` does no I/O.
- **Ordering in SessionExpired:** the reset happens on the callback thread before the persist op is spawned, so the UI never shows SignedOut with stale offers. Fine.

## New (Low)
- **P3-1. The History staleness check is not atomic with the write.** `history_ops.rs:85` checks `gate.is_current`, then builds every row (`row_of`, up to 200 filesystem `complete_entries` reads plus a device-list clone per row), then writes `rows` at line 94 without checking again. A reset plus invalidate that lands inside that window still repopulates the old account's rows. The view is hidden while signed out and the next sign-in reloads it, so the impact is small. Fix: check `is_current` again under the `history` lock just before `rows = rows`. Optionally invalidate before `files.reset()` in the helper.
- **P3-2. Comment formatting.** The `sweep` doc comment in `retention.rs` has a 140+ character line and a stray mid-sentence break ("A missing root is not an / error"). Cosmetic.

## Recommended
1. P3-1: re-check `is_current` before writing rows (2 lines).
2. Optional: rewrap the `sweep` doc comment.

## Unresolved questions
- None new. Still open from pass 2: a separate size cap for explicit downloads, deferred to 04b.
