# Review pass 3: phase 04b fix round 2

Score: **9 / 10** (was 8.5). Critical: 0. High: 0. Medium: 0. Low: 5 (all optional).

Checks: `cargo test --workspace` green (app 64, platform lib 123, ipc 10, all integration suites ok). Clippy `--all-targets` reports 0 warnings and 0 errors on default and on `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`.

## N1-N8 verification

| Item | Status | Evidence |
|---|---|---|
| N1 every `last_event` change counts | Fixed | `StatusCell::update` (`platform/src/sync/status.rs:56-66`) compares before and after under the lock and bumps. `event()` double-bumps, which is harmless because the counter is monotonic. The real inbound test `a_slow_publish_outcome_does_not_overwrite_a_received_clip_status` (`tests_outbound.rs:183`) drives `on_remote_clip` during a 600 ms publish. See P1 for one residual edge |
| N2 request sequence | Fixed | `Staged::begin_request/is_latest/void_requests`. The request is taken in `stage_and_show` *before* spawn, and `stage_with` checks `is_latest` under the `pending_send` lock before set or clear. Equality on a monotonic u64 is correct, and `void_requests` invalidates every older request. Sign-out calls it (`inbox_runtime.rs:98` → `reset_for_sign_out`, all sign-out paths via `reset_account_files`) |
| N3 cold start | Fixed | Pure `send_validate::cold_start`. State transitions are direct (`ops_session.rs:120-151`, `boot.rs:115-123`): no transient SignedOut/NeedsPairing while booting a signed-in device, so immediate refusals are safe. Each state has its own sentence |
| N4 reopen by id | Fixed | `send_files.rs:83` reopens only when `staged_id == current id` |
| N6 STALE reopen | Fixed | `send_files.rs:24-26`: `show_section(None,"send")` with the current set |
| N5 shutting-down + cap | Fixed | `Disconnected` → `shutting-down`. `Inflight` RAII. The slot moves into the op closure, so it is released on normal return, on a panic (unwinding inside `catch_unwind` drops `_slot`), on `spawn` declining because the gate is closed (the closure is dropped), and on a thread-spawn error (`Builder::spawn` drops the closure). `take` uses fetch_add then a compare, so there is no TOCTOU. `busy` has its own text |
| N7 split | Fixed | `send_files.rs` 131, `send_stage.rs` 179, `send_validate.rs` 164 lines. The tests file is 359 lines (tests; under the 500 limit) |
| N8 client non-UTF-8 | Fixed | `forward` pre-checks `args.send` and prints `path-unsupported` with exit 1. `to_line` refuses as a second line of defence |

Regression scan:
- Sequence comparisons are all under the `pending_send` lock.
- `SUPERSEDED` is silent: no banner, and the IPC reply is ok.
- A refusal by the latest request clears the shown set. That was intended and accepted earlier.
- A sign-out during an in-flight `pick` send: core fails without a session, the id has changed, so the picker does not reopen.
- Nothing reaches core unvalidated. Drops, IPC, cold start and pick all go through `validate`, and the IPC protocol still refuses NUL and relative paths.

## Remaining (low, optional)

- **P1** `update` counts only *changes* of text. If a second identical message arrives during a slow publish (for example "Received text from Phone." twice), it does not bump the counter, so the publish outcome overwrites it. The user loses the cue for a new receive. Fix: an explicit `StatusCell::set_event` that always bumps, used by the inbound writers; or bump on any write to `last_event`.
- **P2** Cap scope: only staging counts toward `MAX_INFLIGHT`. Row drops (`send_files.rs:103`) and `pick` re-validation each spawn an uncounted thread that can block on a dead mount; they are user-paced, so this is minor. Also, four checks stuck on a hung mount make every later `--send` or window drop answer `busy` until restart. Document this, or let `busy` say "a drive is not responding".
- **P3** A request that is still latest can finish minutes later (mount recovers) and pop the picker out of context. Consent holds because the summary is shown. Optional: ignore verdicts older than about 60 s.
- **P4** If the launcher's cwd is not UTF-8 and a relative path is given, `forward`'s pre-check passes, `to_line` then fails, and the user sees "cannot reach the running instance: ... path-unsupported". Run the pre-check on `std::path::absolute(p)`.
- **P5** (carried N9) A save writes back the stale loaded baseline if core changed the value since the screen loaded. Accepted.

## Notes
- L4 (picker lists unpaired devices) is still acceptable: core refuses before any data moves.
- QML drop runtime (`QList<QUrl>`) and the Dialog behaviour were checked only in the offscreen and container smoke; still not tested interactively.
