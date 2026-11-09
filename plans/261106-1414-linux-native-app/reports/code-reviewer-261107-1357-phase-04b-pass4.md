# Review pass 4: phase 04b fix round 3

Score: **9.5 / 10**. Critical: 0. High: 0. Medium: 0. Low: 0 open (P2/P3 accepted and documented).

Checks: `cargo test --workspace` green (app 65, platform lib 124, ipc 10, all integration suites ok). Clippy `--all-targets` reports 0 warnings and 0 errors on default and on `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`.

## Verification
- **P1 fixed.** `StatusCell::update_event` (`platform/src/sync/status.rs:57-64`) bumps the counter on every call, under the `inner` lock. Writer inventory (grep of `last_event` assignments outside tests):
  - `event()` → `update_event`
  - `shared_inbound.rs:61` (auto applied), `:76` (waiting for Paste), `:120` (connection lost) → `update_event`
  - `engine.rs:171` (Pasted) → `update_event`
  - `event_if_unchanged` (the guarded publish outcome) → `update` with its own bump
  - The remaining `update` callers (`engine.rs:84` paused, `shared.rs:57` auto) do not touch `last_event`. `update`'s change detection still catches a future writer that forgets `update_event`.
  - No writer bypasses the counter. Every bump and every read inside `event_if_unchanged` happens under the same mutex, so the compare-and-write cannot interleave. Double bumps (`event_if_unchanged` inside `update`) are harmless because the counter is monotonic.
  - A new test covers a second identical "Received ..." during a slow publish.
- **P4 fixed.** `startup.rs:33-37` `send_paths_are_utf8` checks `std::path::absolute(p)`, so a non-UTF-8 cwd with a relative path gives `path-unsupported`, exit 1. Unit test included. Trivia: `--send ""` (absolute fails) also prints `path-unsupported`; that is harmless.
- **P2/P3** are documented in ADR 014 "Round 3 notes" (busy after 4 stuck checks until restart; row drops and pick re-checks are not capped; a late-latest stage may open the picker out of context while the summary still informs the user). Accepted.
- **No regressions** found in send staging, picker or IPC paths since pass 3 (no changes there beyond the launcher pre-check).

## Remaining notes (not findings)
- L4 (picker lists unpaired devices; core refuses) accepted.
- Drag-and-drop from real file managers and the picker Dialog behaviour are still not exercised interactively (offscreen and container smoke only). This is a manual check for the Omarchy run.
