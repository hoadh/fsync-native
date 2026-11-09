# Code review: phase 04a (files inbox, retention, offers, history copy-back)

Score: **7.5 / 10**. Critical: 0. High: 1. Medium: 5. Low: 8.

## Scope
- platform/src/files/{mod,inbox,inbox_tests,retention,download_dir,transfer_wait}.rs, sync/{shared_files,inbound,lanes,shared,shared_inbound,engine}.rs, clipboard/{uri_list,formats,x11_conn}.rs
- app/src/engine/{inbox_runtime,offers,history_ops,xdg_open,clip_service_adapter,e2e_control,clipboard_runtime,shutdown,ops}.rs, core_state/{files_state,hub}.rs, bridge/{history_model,transfers_object}.rs, qml/{HistoryView,OffersView,Main}.qml
- Cross-checked core: engine/place.rs (fetch_file_to containment), service/transfers_inner.rs (final events), service/convert.rs (total_size = sum of declared sizes), settings.rs (retention defaults 30 d / 1 GiB)
- Verified: `cargo test -p fsync-linux-platform --lib` 110/110 green. apps/linux is untracked, so there is no git diff; files were read directly.

## Overall
Path handling is solid and defended twice: the item id regex, a component check on every rel_path, core's `create_dirs_in` + canonical root + never-overwrite, a check that the placed path's top component is under the dir, a symlink-free `fresh_dir`, and `symlink_metadata` in retention. The transfer waiter correctly keeps a final event that arrives before the wait starts. Threads follow the rules: core callbacks only queue or take short locks, network work runs on op/lane threads, and QML updates go through `qt_thread().queue`. The e2e guard is intact (module behind `cfg(feature)`, compile_error in main.rs plus FSYNC_ALLOW_TEST_FEATURES). No file names or content are logged. The main problems are concurrency between explicit downloads, retention and in-flight downloads, and lifecycle (sign-out, unbounded offers).

## Critical
None.

## High
**H1. Two downloads of the same large clip run at once and destroy each other's inbox dir.** `history_ops.rs:174-183`, `inbox_runtime.rs:132-156`, `inbox.rs:166-186` (fresh_dir), `inbox.rs:157-160`, `HistoryView.qml:139-143`.
The Download button stays visible and enabled until the next reload (`can_download` only flips after the download ends). Nothing guards against a second request: no in-flight set, no gate. Scenario: the user double-clicks Download on a 2 GiB clip.
1. T2 `fresh_dir` calls `remove_dir_all(<inbox>/<id>)` while T1 is pulling into it, then recreates it.
2. T1's core export lands in T2's new dir (same path) or fails on the canonicalize.
3. When either one fails, its error branch `remove_dir_all(canonical)` deletes the other's partial or even finished copy, including the marker.

Result: both fail or the folder is left in a bad state, and up to twice the bandwidth is spent. The same happens if the e2e `history-download` and the UI overlap. Fix: keep a `Mutex<HashSet<String>>` of item ids in flight in `FilesState`. Refuse a second download with "Already downloading", show a busy state on the row (`downloading` role, button disabled), and remove the id when done. Optionally route all inbox writes per item through one lock.

## Medium
**M1. Retention deletes a large clip the user just downloaded, or one still downloading.** `inbox_runtime.rs:159-180`, `retention.rs:97-105`.
- `keep` only holds `current_file_item()`. `download_blocking` never sets it and is never added to keep.
- A large clip over 20 MiB fits under the default 1 GiB budget only if it is small. A 1.5 GiB download makes `total > max_bytes` at the next pass (start, 24 h tick, `retention_soon`), and the sweep deletes it oldest-first.
- Because it is the newest dir, it goes last, but it still goes, since it alone exceeds the budget.
- If the 24 h tick fires mid-download, the partial dir is deleted, the core export fails, and the user sees a failure.
- Scenario: the user downloads a 1.5 GiB clip, xdg-open shows the folder, the user restarts the app, and the folder is gone.

Fix: add item ids being downloaded to `keep` (via the H1 in-flight set). Decide on a policy for explicitly downloaded large clips: exempt them until they age out by days, or document it in ADR 014.

**M2. Pending offers grow without bound.** `offers.rs:226-258`.
The trim loop `break`s when no finished offer exists, so a paired peer (or a buggy one) sending many pushes adds unlimited Pending rows. Each also gets its own op thread plus a desktop notification. The GUI clones the whole list on every 4/s poke. Fix: when the list is over MAX_OFFERS with no finished row, drop the oldest Pending row (decline it), or refuse the new one. Rate-limit the notifications too, for example by coalescing within N seconds.

**M3. Offers, held large clips and History rows survive sign-out** (accepted-risk judgment: not fully acceptable). `ops_logout.rs:49-57` does not touch `core.files`.
- After sign-out, sign-in to another account on the same machine still shows the previous account's sender device names and a "Offers (N)" badge. Accept then fails with an unclear core error, or worse, still talks to a peer core remembers.
- An auto-accepted push still Receiving keeps its op thread waiting for up to 24 h (`PUSH_BUDGET`). It is not cancelled.
- `HistoryUi.rows` (clip previews = content) stay in memory. `load_blocking` returns early without a service, so they are only replaced on the next successful load.

Fix: in logout, set every Receiving offer's cancel flag, clear `offers`, `large` and `history.rows`, then poke both models. This takes a few lines and closes a real privacy gap on shared desktops.

**M4. The QML delegate declares `required property string state` on an Item.** `OffersView.qml:31`.
This shadows `Item.state`, the state-machine property. It works on Qt 6.2 because shadowing a non-FINAL property is allowed, but qmllint flags it. Qt 6.10+ warns about shadowing without `override`, and any future FINAL marking breaks the whole Offers view: `row.state` would read Item.state, all buttons would hide, and a "State pending does not exist" warning would appear. Fix: rename the role to `offerState` in `transfers_object.rs` ROLES and in the QML.

**M5. Shutdown does not cancel offer or history downloads.** `shutdown.rs:108-175`, `offers.rs:361-399`, `inbox_runtime.rs:141`.
- Only the inbound lane is cancelled (via Lane generation). Op-thread transfers (accept_blocking, download_blocking with `&|| false`) get no signal.
- `svc.stop()` calls `abort_all` and drops the core tasks without final events, so those waiters spin with 100 ms slices until 24 h / 30 min. They do not block exit, because the threads are detached and the process ends after quit.
- Not a deadline breach, but against the spec ("downloads cancelled/joined within 6 s"): partial inbox dirs are left without a marker (they are cleaned on reuse or by retention), and a waiter can run during `set_service(None)`.

Fix: add a `cancel_all()` on FilesState that sets all offer cancel flags plus a shared "download cancel" flag passed to download_blocking. Call it right after `gate.close()`.

## Low
- **L1. File-size target.** `offers.rs` 355 lines, `history_model.rs` 234, `inbox.rs` 243, `inbox_tests.rs` 239, `retention.rs` 203, `formats.rs` 201 are over the 200 target, all under 500. Moving the offers tests to `offers_tests.rs` gets it to about 255.
- **L2. A remote file named `.complete.tmp`** (top level) is allowed by `valid_component` (`inbox.rs:68-75`). `write_marker` then truncates and renames it to `.complete`. The listed entry is missing, so the clipboard gets a dangling URI and the item never counts as complete, which means a re-fetch every time. Only the sender's own clip is affected. Fix: reserve the `.complete*` prefix.
- **L3. Failure text from core is shown and logged raw.** `TransferEvent.message` (`FsyncError::to_string()`) goes to the Offers "Failed: ..." line and `tracing::info!("an offer failed: {message}")` (`offers.rs:433`). No file names were seen in that path today, but this should go through `user_messages` for consistency.
- **L4. GNOME copy-back reports "Copied." before the GUI thread writes.** `history_ops.rs:153-158` and 168: `write_from_window` returns true once queued. If `set_entry` then fails, its error message can be overwritten by the later `set_message("Copied.")` (ordering race).
- **L5. Full model reset on every offers poke** (up to 4/s) recreates delegates, so a Cancel press during progress can be lost and scroll resets. History also resets on every `lastEventChanged` while visible. Consider `dataChanged` for progress.
- **L6. `TransferWaiter` lives on `Core`, but transfer ids are per `FsyncService`.** If a boot retry ever replaces the service in-process, stale finals (for example a Cancelled one left after an Aborted wait) could match new ids. This is unlikely today because the service is set once. Clear the waiter in `set_service`.
- **L7. In Manual mode `current_files` points to the *pending* item, not the one actually on the clipboard.** Retention may remove the previously pasted files while they are still the clipboard content. Copy-back has a small time-of-check/time-of-use gap between `complete_entries` and a concurrent sweep. Both have low impact.
- **L8. Deleting or clearing History does not remove the matching inbox dirs.** Files stay until retention, which may surprise a user who expects "delete" to remove the received files. Document it or delete on `history-delete`.

## Edge cases checked, OK
- Traversal: rel_path `..`, empty components, `/`, NUL, CR/LF and 255-byte names are refused. Core re-validates and refuses symlinked parents. An `<id>` that is a symlink or plain file is unlinked, never followed (tested).
- Retention: links and stranger names are skipped, inner links are counted as links, and `remove_dir_all` does not follow. The current item is kept even when old or over budget (tested).
- Newest-wins: a new inbound clip bumps the lane generation. The waiter polls cancel every 100 ms, calls `svc.cancel(id)`, and the partial dir is removed. A final event that arrives before the wait is kept (bounded to 256).
- Eager threshold: `total_size > 20 MiB` means large, so exactly 20 MiB is eager, matching the spec. total_size is the core-computed sum of declared sizes, and blob hashes bound the real bytes.
- Auto-accept: requires the setting and `total <= 1 GiB`. A failed settings read means no auto-accept. Accept is idempotent via the Pending -> Receiving switch under the lock.
- Echo: `files_entry` and `to_entry` carry MARKER_MIME. `sh.write` remembers the guard. Copy-back sets `current_files` before the write.
- Threads: callbacks (`on_transfer`, `on_incoming`, `on_large`) do no blocking work. The GUI-thread invokables only take short locks or spawn. Models reload from snapshots on the GUI thread.
- x11_conn retry: at most 3 retries (0.9 s), only on reset, refused or EOF, and run on the boot thread. Shutdown's 0.5 s boot wait only warns.
- Download dir: `$HOME` must be absolute, a relative XDG value falls back, `create_private_dir` gives 0700.
- e2e guard: the module, protocol and handler are all `cfg(feature = "e2e-control")`, and a release build is blocked without FSYNC_ALLOW_TEST_FEATURES.
- Qt 6.2: `QtQuick.Controls.Basic` and required properties are fine. A smoke run in the 22.04 container was reported.

## Recommended actions
1. H1 + M1: in-flight download set (refuse duplicates, disable the button, add to retention keep).
2. M3: clear and cancel offers, large clips and history rows on sign-out.
3. M2: bound Pending offers and coalesce notifications.
4. M4: rename the `state` role to `offerState`.
5. M5: cancel op-thread downloads in shutdown right after the gate closes.
6. L2 (reserve the `.complete*` prefix), L1 (split the offers tests).

## Unresolved questions
- Should an explicitly downloaded large clip be exempt from byte-based retention (Windows parity, d13)?
- Is the core offer state per account? Can an old account's offer still be accepted after sign-in as someone else?
