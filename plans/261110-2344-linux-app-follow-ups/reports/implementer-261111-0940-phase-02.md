# Implementer report: phase 02 (offline-publish lows + FFI contract 3)

Branch `fix/offline-publish-lows` (from main 381de6e). Not pushed.

## Changes
- **L2** `core/fsync-core/src/clip/peer_send_lanes.rs`: the per-peer ticket queue is replaced by `PeerSendLanes<T>`, which keeps one "latest" slot per lane: `{ running, waiting: Option<T> }`. `submit` returns `Run(runner, job)` when the lane is idle, or `Wait { replaced }` when it is busy. A newer clip replaces the waiting one, and the running attempt keeps its own frame. `LaneRunner::next()` takes the waiting clip or sets the lane idle. If the runner is dropped without draining (the task was aborted on close, or `spawn` refused it), it frees the lane and drops the waiting clip, so its `rx` gets "no answer", as before.
- `clip/send.rs`: `SendJob {item_id, frame, tx}`, `run_lane` (sends the job, then whatever waits), and `finish_send` (logs, unoffers on failure, reports). A replaced clip is finished at once as `Net("superseded by a newer clip")` with an unoffer, the same outcome as before. Pushes still bypass the lane.
- **L1**: doc comments on `Shared::publish`, `Engine::publish_clip` and `FsyncService::publish_clip` say that publishes must be serialized for ordering. `PublishInFlight` counts running publishes (`Shared::publishing`) and logs at debug level when two overlap. No locking change.
- Test hook (feature `test-hooks` only): `Shared::frames` records a Weak per built frame, and `Engine::clip_frames_held()` counts the live ones.
- **contract_version 3**: `core/fsync-ffi/src/lib.rs` now returns 3, with an updated history comment. Windows `App.ExpectedContract`, `EngineContractTests.cs` and `FsyncCoreKitTests.swift` now expect 3. New macOS check: `EngineBridge.expectedContract` (3), compared at the top of `makeEngine()` in `EngineBridge+Boot.swift`. On a mismatch it sets `.bootFailed("fsync could not load its engine: fsync_ffi contract N, app expects 3", canReset: false)`, the same wording as Windows.
- **Bindings**: regenerated on Linux with the same tools as `gen-bindings.sh` / `gen-bindings-cs.ps1` (uniffi-bindgen from the repo with `--features bindgen`; uniffi-bindgen-cs v0.10.0+v0.29.4 installed under `~/.cache/fsync-p02-bindgen-cs`; `--library libfsync_ffi.so`, the same uniffi.toml and `--no-format`). The content DID change: UniFFI's function checksum covers the doc comment, so the `contract_version` docstring and checksum (18762 -> 64396) changed in both files. Everything else is byte-identical to the checked-in files, which shows that Linux generation matches the macOS/Windows output. `gen-bindings.sh` itself was not changed.
- **Docs**: ffi-contract (v3, startup checks, lane semantics, serialized publishes), protocol-spec step 3, system-architecture (contract v3, lane), codebase-summary (v3), changelog, roadmap.

## Tests
- `cargo fmt --all --check`: OK. `cargo clippy --workspace --all-targets -D warnings`: OK. `cargo clippy -p fsync-core -p fsync-ffi --lib -D warnings` (production features, no test-hooks): OK.
- `cargo test -p fsync-core -p fsync-ffi --no-fail-fast`: 252 passed, 0 failed, 3 ignored (51 binaries, Docker available).
- `clip_offline_peer_publish`: 6/6 passed in 5 more consecutive runs (about 8.1 s each), including `slow_live_peer_gets_the_newest_clip_never_an_older_one_after_it` and the new `a_burst_to_a_dead_peer_holds_at_most_two_frames` (50 publishes to a dead peer, `clip_frames_held() <= 2`, 0 after close).
- New lane unit tests (4): the newer clip replaces the waiting one; peers are independent; a 50-clip burst holds 2 frames (Arc count 3); an aborted runner frees the lane and drops the waiting clip.
- Swift (`swift test`) and C# (`dotnet test`) were not run here. They need the macOS and Windows CI jobs.

## Mutation
- Scratch copy `~/.cache/fsync-p02-mut` (separate target dir). The old `peer_send_lanes.rs` and `send.rs` from 381de6e were restored, with only the `track_frame` hook kept. Result: `a_burst_to_a_dead_peer_holds_at_most_two_frames` FAILS with "50 frames held for a dead peer". The other 5 tests, latest-wins included, pass, so the new test is what catches the unbounded queue.

## Commits
- 11701e8 fix(core): keep one latest clip per peer send lane; log overlapping publishes
- 17d8adf feat(ffi): bump contract_version to 3 and check it at macOS startup
- (docs commit follows: docs + this report)

## Unresolved questions
- The macOS startup check and the Swift/C# tests are unverified locally. The macOS and Windows CI jobs must be green (the plan's gate).
- The scratch dirs `~/.cache/fsync-p02-{mut,mut-target,bindgen-cs,bindings,tmp}` are left in place. Delete them when they are no longer needed.
