# Review pass 4: phase 04a, coordinator fix of P3-1 / P3-2

Score: **9.5 / 10** (pass 3: 9). Critical: 0. High: 0. Medium: 0. New: 1 cosmetic note.

## Verification
- `cargo test --workspace` (apps/linux), rerun by the reviewer: all green. Platform lib 120, app 37, integration suites 2/2/3/5/7/7/8/9.
- Clippy (both feature sets), fmt and the 22.04 smoke run were reported clean by the coordinator. I did not rerun them this pass; the only changes since pass 3 are about 10 lines.

## P3-1: the ordering argument holds
Setup:
- **Load (L)**, in `history_ops.rs load_blocking`, Ok branch: build rows, lock `history` (H), check `is_current(History, g)`, write rows, unlock.
- **Reset (R)**, in `reset_account_files`: `invalidate(History)` (SeqCst `fetch_add`), then `files.reset()`, which locks H and clears rows.

Proof:
- **Case 1: L's under-lock check reads the counter before R's increment.**
  - If R already held H, then R's increment came before R's lock, which came before L's lock, which came before L's check. L would see the bumped counter, a contradiction.
  - So R has not taken H yet, or L holds H. L writes its rows, unlocks, and R then locks H and clears them. The end state is empty.
- **Case 2: L's check reads the counter after R's increment.** The check fails, L returns, and nothing is written. The end state is empty.
- **Conclusion:** both cases are covered. SeqCst atomics plus mutex happens-before make the order well defined. A load that starts after the reset begins a new generation and legitimately reads the (signed-out) service; the view is hidden and the next sign-in reloads.

Leftovers (cosmetic, no fix needed):
- The `Err` branch writes `h.message` after only the first, unlocked check. A reset in that window can leave a generic error sentence (no account data) in an otherwise cleared state. The next load overwrites it.
- The stale early return in the Ok branch skips `poke_history`. That is harmless: `loading` was already set false under the first lock, and reset pokes on its own.

No deadlock risk: the gate counter is an atomic and is never read while waiting for H. Both functions take only H, and reset takes its other locks (offers, large) one at a time, each released before the next.

## P3-2
FIXED. The `retention.rs` `sweep` doc is reflowed (lines 75-78).

## Regression scan
- `load_blocking` still sets `loading` true and false on the same paths, and the first check is unchanged.
- `reset_account_files` callers (logout on the GUI thread, SelfRemoved and SessionExpired on the core callback thread) are unchanged. The added invalidate is a single atomic add, which is non-blocking and safe on the callback thread.
- No other files changed.

## Still open (accepted)
- L1: file-size target. `retention.rs` 279, `inbox.rs` 258 and `inbox_tests.rs` 313 lines are over 200 and under 500.
- L6, L7, L8 as in earlier passes.
- A separate size cap for explicit downloads is deferred to 04b.

## Unresolved questions
None.
