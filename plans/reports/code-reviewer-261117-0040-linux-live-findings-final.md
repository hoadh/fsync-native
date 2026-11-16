# Code review: d6228e0 (PR #7, fix/linux-live-findings), final round

Score: **9.5/10, pass.** Earlier rounds: 6.5, 8.5, 9.0.

## Scope
`apps/linux/platform/src/sync/{shared.rs,shared_inbound.rs,engine.rs,tests_repeat.rs}`. Repeat window measured
between watcher sightings (`OutJob::Auto(change, Instant)`) instead of at admission.

## Verification
- `cargo test -p fsync-linux-platform sync`: 39/39 pass (8 in `tests_repeat`).
- `cargo clippy -p fsync-linux-platform --all-targets`: clean.
- Live 2026-11-17 on d6228e0: one Ghostty copy, one receipt on the CLI peer (3/3 duplicated before).

## Findings
Critical / High / Medium: none.

Low:
1. The sighting stamp was taken after `service.settings()` (a blocking store read). A wait there behind a running
   publish would stretch the gap again. **Fixed:** stamped on arrival at the top of the `Ok(change)` arm.
2. Two docs (`AUTO_REPEAT_WINDOW`, `last_auto`) still said "time of the last automatic publish". **Fixed.**
3. `a_publish_slower_than_the_window_does_not_let_the_second_sighting_through` only discriminates when the first job
   is admitted before the second copy arrives (20 ms gap); on a very loaded runner it can pass vacuously. It cannot
   fail falsely. Left as is.

## Notes
- `Instant` is monotonic and all stamps come from the single watcher thread; the out lane is FIFO with one worker.
- Cancel check and record share the `last_auto` lock: a job either records and sends, or does neither.
- A genuine re-copy of the same content within 1 s is dropped; peers already hold it unless the first publish failed.

## Unresolved
- If a clipboard-persist tool re-owns the selection later than 1 s (e.g. on Ghostty window close), it would publish
  again. Not seen on Omarchy; out of scope.
