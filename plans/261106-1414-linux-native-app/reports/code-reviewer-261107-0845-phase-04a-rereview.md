# Re-review: phase 04a, fix round 1

Score: **8.5 / 10** (previously 7.5). Critical: 0. High: 0. Medium: 0. Low: 9, of which 5 are new, all minor.

## Verification (run by the reviewer)
- `cargo test --workspace` (apps/linux): all green. Platform lib 119, app 35, integration suites 2/2/3/5/7/7/8/9.
- clippy `--workspace --all-targets`, default and with `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`: 0 warnings, 0 errors in both runs.

## Findings from round 1
| ID | Status | Evidence |
|---|---|---|
| H1 double download | FIXED | `inbox.rs:74-129`: the shared `in_flight` set and `DownloadGuard` (RAII release). One `Inbox` lives on `Core`, and the clipboard engine gets a clone that shares the Arc (`core_state/mod.rs:105`, `clipboard_runtime.rs:72`). The History, eager and e2e paths all claim the item first. The row gets a `downloading` role. Test `a_second_download_of_the_same_item_is_refused_until_the_first_ends`. |
| M1 retention vs downloads | FIXED | `inbox_runtime.rs:96-102`: keep = current item + `downloading()`. `.explicit` is written while the claim is still held (`inbox_runtime.rs:71`). `retention.rs:100-110` leaves explicit items out of the byte total and out of size-based removal; the age limit still applies. Documented in ADR 014 (lines 230-233). |
| M2 unbounded offers | FIXED | `files_state.rs admit_offer`: at most 20 active offers and 50 rows. Overflow is booked as a Declined row on the callback thread, with no thread and no notification. Notifications are coalesced over 5 s (`notify_due`). |
| M3 sign-out leftovers | MOSTLY FIXED | `FilesState::reset()` (cancel flags, epoch bump, clear offers/large/history) runs on logout (`ops_logout.rs:23`, GUI thread, short locks) and on SelfRemoved (`ops_account.rs:51`). Residual issues: L-N1 and L-N2 below. |
| M4 `state` shadowing | FIXED | Role renamed to `offerState` (OffersView.qml:31). No other delegate role shadows an Item property. |
| M5 shutdown cancellation | FIXED | `shutdown.rs:116` `cancel_all()` runs right after `gate.close()`. Offer waiters see their flags, and `download_blocking` watches the epoch. Within 100 ms they abort, call `svc.cancel(id)` while the service still exists (the engine stops later), and the partial inbox dir is removed. The threads are still not joined, which is fine because they are detached and stay within the deadline. |
| L1 file sizes | PARTIAL | `offers.rs` was split to 177/87/94. Still over the 200 target, all under 500: `inbox.rs` 325, `files_state.rs` 323, `inbox_tests.rs` 313, `history_model.rs` 266, `retention.rs` 235, `transfers_object.rs` 233. |
| L2 `.complete.tmp` | FIXED | `is_reserved` treats the `.complete*` and `.explicit*` prefixes as reserved (test `marker_names_are_reserved`). |
| L3 raw core text | FIXED | `clip_service_adapter.rs`: `Outcome::Failed(_)` becomes the fixed sentence "The transfer failed." The info log of the message is gone. |
| L4 GNOME "Copied." race | FIXED | `CopyOutcome::Queued` sets no message. `HistoryModel::write_entry` sets "Copied." or the failure message on the GUI thread. |
| L5 model reset churn | FIXED | Both models emit `dataChanged` (all roles) when the row ids and order are unchanged, and reset otherwise. Required-property delegates update in place. |
| L6 waiter vs service restart | OPEN (accepted) | The service is still set once per process. |
| L7 Manual-mode `current_files` | OPEN (accepted) | |
| L8 History delete keeps inbox copies | OPEN (documented) | |

## New / residual issues (all Low)
- **L-N1. SessionExpired does not reset FilesState.** `ops_account.rs:67-75` moves to SignedOut without calling `core.files.reset()`. On a shared desktop, the next account to sign in still sees the previous account's offers and History rows (previews). An accepted push keeps running until it ends. Fix: call `core.files.reset()` plus both pokes in that arm too, as for logout and SelfRemoved.
- **L-N2. An in-flight History load can bring old rows back after reset.** Logout invalidates `OpKind::Account` but not `OpKind::History`. A `load_blocking` that started before `reset()`, or the `load_blocking` after a cancelled download (`history_ops.rs` `download`), writes rows again once reset has cleared them. The view is hidden while signed out and refreshes on the next sign-in, so the exposure is small. Fix: `core.gate.invalidate(OpKind::History)` inside the reset path.
- **L-N3. `history_ops::download` maps every `begin_download` error to "Already downloading."** That includes an invalid item id. Use the `FetchError::Failed` text.
- **L-N4. The retention keep list is a snapshot.** A download claimed after `downloading()` was read but before `sweep` reaches it is not protected. That dir is brand new, so only the byte pass can hit it, and only if it is the last non-explicit dir and still over budget. The window is very narrow. Acceptable, or take the in-flight lock for the whole sweep.
- **L-N5. Explicit downloads are bounded only by age.** With 30 d and multi-GB clips, the inbox can grow well past `retention_bytes`. This is by design (ADR 014) but worth a sentence in the History or settings help text in 04b.

## Regression checks (no issue found)
- **DownloadGuard:**
  - It is dropped on every path: early return, the `?` in `download_blocking`, the panic inside the `ops::spawn` catch_unwind (unwinding drops it), and the e2e guard moved into the closure (dropped if spawn is refused).
  - The guard id is checked against `item.item_id` in `fetch_item_guarded`.
  - The eager lane goes through `fetch_item`, which claims too. Newest-wins cancel still releases the claim.
- **Epoch:** captured inside `download_blocking` after the claim. A reset in between makes `large_item` return None ("no longer available"), so no stale download runs.
- **admit_offer** runs on the core callback thread with one short lock and no I/O. The duplicate check also covers the Declined overflow rows, so a re-sent overflowed offer is not re-added.
- **accept_blocking after reset:** the row is gone, so it returns early. An accept that was already Receiving sees its flag and records nothing, because the row was removed.
- **`.explicit`** is written only after a successful fetch (into the canonical dir via `item_dir`; the name is reserved, so it cannot be remote-planted). `complete_entries` and the uri-list ignore it.
- **dataChanged:** rows are swapped before the emit, indexes come from the inherited `index()`, an empty role list means all roles, and the GUI thread is the only writer.
- **x11 retry** is unchanged from the previous review. The `is_transient` test was added.
- **e2e guard** is intact. `history-download` refuses duplicates.

## Recommended (optional, non-blocking)
1. L-N1 + L-N2: reset on SessionExpired, and invalidate the History generation in `reset`.
2. L-N3: pass the real refusal text.
3. Later: split `files_state.rs` (admit/notify helpers) and `inbox.rs` (guard and markers) toward the 200-line target.

## Unresolved questions
- Should explicit downloads have their own size cap, separate from history `retention_bytes`?
