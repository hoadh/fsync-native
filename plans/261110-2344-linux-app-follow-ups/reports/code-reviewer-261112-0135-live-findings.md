# Code review: fe2db13 (LIVE findings F1-F3)

Branch `fix/linux-live-findings`, commit `fe2db13`, reviewed at HEAD 39c158c. Read-only.

## Scope
- Files: `platform/src/sync/{shared,engine,mod,tests_repeat}.rs`, `app/src/engine/{tray,shortcut_help}.rs`, `apps/linux/README.md` (+209/-26)
- Context read: `sync/lanes.rs`, `sync/shared_inbound.rs` (watcher, apply), `sync/outbound.rs`, `sync/tests_rig.rs`, ksni 0.3.7 `service.rs` / `dbus_interface.rs` / `lib.rs`, Quickshell `dbusmenu.cpp` (upstream master), `/usr/share/omarchy/shell/plugins/bar/widgets/Tray.qml`, `/usr/share/omarchy/default/hypr/helpers.lua`, `~/.config/hypr/bindings.lua`

## Score: 6.5 / 10

F3 is correct. The F1 dedupe is a reasonable approach, but its "a cancelled job never hides the change that replaced it" claim is not atomic: a lost copy is possible. F2 works for a click on Pause, but the commit's premise ("every state change is a layout change, so every host refreshes") is false. The app never calls `Handle::update` and does not implement `menu_about_to_show`, so ksni only re-reads `menu()` after a menu click. The stated root cause (Quickshell ignores `ItemsPropertiesUpdated`) also conflicts with Quickshell's source.

## High

**H1. A copy can be dropped entirely: the fingerprint is recorded, then the publish is cancelled** (`platform/src/sync/shared.rs:70-77`, `:85-97`, `:126-128`)
The order is `is_cancelled()` check, then `repeats_last_auto()` (hashes the clip, then **records** `(fp, now)`), then `publish()`, which checks `is_cancelled()` **again** and returns silently.
Failure scenario: an app sets the same selection twice (the F1 pattern) and the second `submit` lands after A1's first cancel check but before the check inside `publish`. The window includes the SipHash of the whole clip: microseconds for text, several ms for a multi-MB image. A1 records fp, `publish` sees cancelled and returns. A2 collects the same content, `repeats_last_auto` returns true, and A2 is dropped. Nothing is sent, no status, only a debug log. The doc comment on `repeats_last_auto` claims the opposite guarantee.
Fix: hash outside the lock, then check cancel and record inside the lock, and do not re-check cancel after recording:
```rust
Ok(clip) => {
    let fp = fingerprint(&clip);
    {
        let mut last = lock(&self.last_auto);
        if cancel.is_cancelled() { return; }
        if matches!(*last, Some((p, at)) if p == fp && at.elapsed() < AUTO_REPEAT_WINDOW) { return; }
        *last = Some((fp, Instant::now()));
    }
    self.publish_now(clip); // the body of publish() without the cancel check
}
```
If cancel flips after the record, A1 publishes the same content A2 would have sent. That is the same outcome as today when A2 arrives during `service.publish`, which is not cancellable. Add a test with a hook (a barrier in `FakeService` or a test-only pause between record and publish) so the ordering is deterministic.

**H2. The tray menu never refreshes for state changes made outside a tray click** (`app/src/engine/tray.rs:3-8`, `:132-133`, `:178-185`)
ksni 0.3.7 re-reads `Tray::menu()` only in `update()`: `Handle::update`, after a `clicked` event (`service.rs:536-540`), or in `run_about2show_hook` when `menu_about_to_show` is implemented (`service.rs:561-564`). This app parks the handle and never calls `update`. `menu_about_to_show` uses the default, which sets `NO_ABOUT_TO_SHOW`, so `AboutToShow` does nothing. `GetLayout` serves the cached `flattened_menu`.
Failure scenarios, on every host (KDE, Waybar, Quickshell):
- Manual mode, a clip arrives: "Paste received clip" does not appear until the user clicks some other menu row. Before this commit the row was there but stuck disabled, so it was just as unusable; the new design does not fix it. Tray paste does not work in Manual mode. H2 live only checked Open, History, Send and Pause.
- Pause from the window, IPC or settings: the tray still shows "Pause sync [ ]" with no "Sync is paused" row. A click on it calls `toggle_pause`, which **unpauses**, the opposite of what the user meant.
- Tray "Paste received clip": `clip_action` runs the paste on another thread (`clipboard_actions.rs:49`). ksni's post-click `update` runs before `pending` is cleared, so the row stays. A second click answers "Nothing to paste."

Fix: keep the `ksni::blocking::Handle` (for example in `Core`, or behind a channel the tray thread drains) and call `handle.update(|_| {})` from the engine status notify whenever `paused` or `pending` changes. Optionally also implement `fn menu_about_to_show(&mut self) {}`. A non-default body makes ksni run `update_menu` on `AboutToShow`; see its FIXME about ids changing while the menu is open, and keep `Handle::update` as the primary path.

## Medium

**M1. The F2 root cause in the module doc and ADR conflicts with Quickshell's source** (`tray.rs:6-8`, ADR 014 F2)
Quickshell `dbusmenu.cpp` (master) connects `ItemsPropertiesUpdated` to `onItemPropertiesUpdated`, which updates `toggle-state` on existing items. Omarchy's `Tray.qml:746` binds `modelData.checkState` live. The installed `quickshell 0.3.2.r7` may differ; I did not check that tag. The real cause may be something else, for example the QML tearing down the opener on click (`resetTrayMenu`) before the signal arrives. A row change can still fix it, because `LayoutUpdated` makes Quickshell call `updateLayout`, but that is unproven until the live re-check. Keep F2 as "Pending live verification" and reword the doc as a hypothesis, not a fact.

**M2. Some state changes keep the same row count, so no `LayoutUpdated` is sent** (`tray.rs:134-166`; ksni `service.rs:335-357` compares only children index lists)
The row count is `1 + paused + pending`, so (paused, no pending) and (not paused, pending) both have 2 rows. Since refresh happens only per click (H2), one refresh can flip both. Manual mode, paused, a clip arrives (no refresh), the user clicks "Pause sync" to resume: 2 rows become 2 rows, the update is properties-only, and under the commit's own theory Quickshell stays stale. The test `every_state_change_changes_the_rows` skips the (true, Some) state and the paused-vs-pending pair. The pending-label-only change is already accepted in the ADR. If the theory holds, make the count injective (for example, pending adds two rows: the Paste row plus a separator), or fix H2 so each change is pushed on its own.

**M3. The dedupe memory is not cleared when the clipboard or the peers' state changes** (`shared.rs:85-97`, `shared_inbound.rs:55-66`)
`last_auto` is only written by automatic publishes. Within 1 s of publishing A:
- A remote clip C arrives and is applied (Auto). Peers now hold C. The user re-copies A and it is dropped, so the peers keep C.
- An explicit send of B (send_now) publishes B and `last_auto` stays A. A later auto copy of A within 1 s is dropped, so the peers keep B.

The window is short, so the chance is low, but the loss is silent. Fix: set `*lock(&self.last_auto) = None` in `apply`/`write` (any write we make) and after any explicit or `OutJob::Clip` publish.

## Low

**L1. Timing-dependent tests** (`tests_repeat.rs:22-40`)
`the_same_copy_seen_twice_after_the_first_publish...` needs the second copy to be checked within 1 s of the first record, and the test waits at least 300 ms in between. A loaded CI runner that stalls about 700 ms fails it. Every `published(r, 300ms) == 1` assertion fails if a publish takes more than 300 ms to arrive. The first test is otherwise sound: every interleaving gives 1, except the H1 race, which would give 0. Suggest an injectable window (field on `Shared`, test-only setter) or longer waits (for example 2 s, ending on the first receive).

**L2. The fix assumes both Ghostty selections are byte-identical, including the MIME set.** If the first offer has `text/plain` only and the second adds `text/html`, the fingerprints differ and both publish. The live log only shows "two clips". The live re-check should confirm.

**L3. `fingerprint` hashes the whole clip on the lane thread, up to `max_clip_bytes`.** That is fine, and much cheaper than the read and the publish. A `DefaultHasher` collision (SipHash-1-3, fixed keys, 64-bit) between two different consecutive clips within 1 s is about 2^-64 by chance. Forcing one would need control of both clips, which gains an attacker nothing. Acceptable for this purpose. Hashing `Vec<u8>` includes the length, so there is no concatenation ambiguity.

## Answers to the brief
- **F1 lane ordering**: newest-wins plus the pre-record cancel check is correct, except for the H1 window. Manual mode and pause never submit `Auto` (`shared_inbound.rs:107-109`), so they are unaffected. `send_now` and `OutJob::Clip` bypass the dedupe (correct), but they do not update or reset it (M3). Files clips: the fingerprint covers the canonical paths only, not file contents. Re-copying the same paths within 1 s after an edit is dropped, which is unrealistic. The read is cheap (a uri-list only).
- **Read before deciding**: required for content equality. Both copies were already read before this commit, so there is no new cost besides the hash.
- **ksni claim**: confirmed. `update_menu` sets `layout_updated` only when an item's children list differs. For a flat menu that means the root's child indices, which in practice is the row count. Otherwise it emits `ItemsPropertiesUpdated`.
- **Id shift**: safe. On a layout change, `item_id_offset += old_len`, so every old id is `<= new offset`, `id2index` returns None, and the click is ignored ("not found"). A stale id never reaches a different item. The only cost is a lost click if the host clicks with ids from before the signal, which is unlikely because the menu usually closes on click.
- **KDE / Waybar**: both handle `LayoutUpdated` (libdbusmenu-qt / -gtk); the state-built menu is fine there. The remaining staleness there is H2, which is not specific to Quickshell.
- **F3**: correct. `o.bind(keys, description, dispatcher)` in `/usr/share/omarchy/default/hypr/helpers.lua:138` wraps a string dispatcher in `hl.dsp.exec_cmd`. The lines match the user's working `~/.config/hypr/bindings.lua` exactly. The `omarchy menu keybindings --print` hint matches the file's own header.

## Positive
- The explicit/auto split is clean. Dedupe only on the automatic path matches the user's intent.
- The ksni behaviour (offset bump) was checked, and ids cannot misroute.
- The README and help are split per desktop instead of one ambiguous line.

## Metrics
- Tests: `cargo test -p fsync-linux-platform --lib sync::` 36/36 pass (incl. 4 `tests_repeat`), at HEAD 39c158c.
- `cargo clippy --workspace --all-targets`: clean (0 warnings), at HEAD 39c158c.
- `cargo test -p fsync-linux --bins tray::`: 1/1 pass (`every_state_change_changes_the_rows`).
- All runs used `CARGO_TARGET_DIR=/home/alex/.cache/fsync-f-review-target`, which was removed afterwards.

## Recommended actions
1. H1: make record and publish one commitment (code above) and add a deterministic test.
2. H2: push tray updates from the engine status (`Handle::update`). Without it the tray Paste row and pause state are stale on every host.
3. M3: clear `last_auto` on inbound apply and on explicit publishes.
4. M1/M2: treat F2 as unverified until live. Cover all 4 (paused, pending) states in the test.
5. L1: an injectable window or longer waits in `tests_repeat`.

## Unresolved questions
- Does the installed Quickshell 0.3.2.r7 really drop `ItemsPropertiesUpdated`, or does the Omarchy QML tear down the opener on click? This decides whether M2 matters.
- Are the two Ghostty selections byte-identical (MIME set included)?

## Re-review (fe2db13..13bc7a1)

Score: **8.5 / 10**. All earlier High and Medium findings are fixed. Two new Low findings, neither blocking.

| Finding | Verdict |
|---|---|
| H1 lost copy | **Fixed.** `publish_auto` (`shared.rs:110-119`) hashes outside the lock, then checks cancel and records in one `admit_auto` call under `last_auto`. `send` has no cancel check, so a job either records and sends or does neither. If cancel flips after admission, the older content is sent, followed by the newer job: the same as a job that is cancelled during the core publish today. `a_cancelled_job_neither_sends_nor_records` covers the logic deterministically, with no threads. |
| H2 tray never refreshed | **Fixed.** Status notify (`clipboard_runtime.rs:67`) leads to `tray::refresh`, which sets a flag and unparks the thread without blocking. The tray thread swaps the flag and calls `handle.update`. `StatusCell` calls notify after releasing its lock (`status.rs:56-77`), and `refresh` only stores and unparks, so a click callback (`toggle_pause` on the ksni service thread) cannot deadlock. Bursts collapse into one update. A duplicate update after a click emits nothing. Pause from the window or IPC, a received clip and a paste on another thread all reach the tray now. |
| M1 cause wording | **Fixed.** The `tray.rs` module doc and ADR 014 F2 name the missing `update` as the cause and leave the Quickshell part open. |
| M2 equal row counts | **Fixed.** A held clip adds a row plus a separator, giving counts 1/2/3/4. The test asserts all four states. A label-only change of a held clip stays stale on Quickshell, as documented. |
| M3 stale record | **Fixed.** `forget_last_auto` runs in `publish_explicit` (`Explicit` and `Clip`) and in `Shared::write` (inbound apply, paste, history copy-back). Covered by `a_received_clip_resets_the_record` and the new step after `send_now`. |
| L1 timing | **Improved.** A 3 s wait per expected publish removes the false failure when a publish arrives late. Residual risk is noted in R2. |

### New findings
**R1 (Low). A refresh before the tray thread registers itself is lost until the next status change** (`app/src/engine/tray.rs:192-203`).
`tray_thread.set(current())` runs after `spawn()` has built the first menu, and the loop parks before it checks `tray_dirty`. If a status change happens in that gap (for example a held clip or a pause applied during startup), the flag is set but nothing unparks the thread. The thread parks with `tray_dirty == true`, and the menu stays stale until the next status change. Fix: check the flag before parking (`loop { if core.tray_dirty.swap(false, SeqCst) { handle.update(|_| ()); } std::thread::park(); }`), or set `tray_thread` before `spawn()`. Either is a one-line change.

**R2 (Low, tests). The reset and window tests can pass without the fix on a slow runner, and one can fail on a loaded one.**
- `a_received_clip_resets_the_record` and the step after `send_now` only prove the reset if the second copy lands within 1 s of the first record. On a slow runner (each `expect_published` already waits 250 ms, plus the inbound fetch), they pass even without the reset.
- `the_same_copy_seen_twice_after_the_first_publish...` still fails if more than 1 s passes between the first record and the second admission. The margin is about 700 ms.

An injectable window (a `Shared` field the tests set to 10 s) would make all of these exact. This is optional.

### Not re-run
I did not re-run tests or clippy. I relied on the coordinator's results for 13bc7a1: clippy `-D warnings` clean, 241 passed / 0 failed workspace-wide, 38 sync tests passed. No target dir was created this round, so `~/.cache/fsync-f-review-target` is absent.

### Still open
- F1 and F2 still need the live re-check on Omarchy: a Ghostty copy is sent once (confirms L2: both selections identical), and the Pause check and paste row update.

## Re-review 2 (13bc7a1..baaf93f)

Score: **9 / 10**. R1 and R2 are fixed and nothing new was found. The missing point is the open live re-check, not the code.

| Finding | Verdict |
|---|---|
| R1 refresh lost at startup | **Fixed** (`tray.rs:200-205`). The loop now checks `swap(false)` and calls `update` before `park()`. A change made before `tray_thread` was set is picked up on the first pass. A later `unpark` before `park` leaves the park token set, so that change is not lost either. Spurious wake-ups only re-check the flag. |
| R2 timing-dependent tests | **Fixed.** `admit_auto` takes the window, and `Shared.repeat_window_ms` defaults to `AUTO_REPEAT_WINDOW`; `set_repeat_window` is `#[cfg(test)]` only. With a 60 s window, "within the window" holds on any runner, so the reset tests cannot pass without the reset (the coordinator checked this by making `forget_last_auto` a no-op). The expiry test (50 ms window; at least 250 ms of `expect_published` plus a 100 ms sleep) only gets more certain under load, because elapsed time only grows. |

Production behaviour is unchanged: the window is 1 s from `AUTO_REPEAT_WINDOW`, and the `Relaxed` load of a value that never changes outside tests is fine.

Not re-run: I used the coordinator's baaf93f results (clippy `-D warnings` clean, workspace 241 passed / 0 failed). No target dir was created.

Open: the live re-check of F1 (a Ghostty copy is sent once) and F2 (the Pause check mark and paste row update on Omarchy).
