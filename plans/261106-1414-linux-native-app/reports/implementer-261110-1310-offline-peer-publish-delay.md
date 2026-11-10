# Offline peer delays every clip publish

Branch: worktree-agent-aba5a088e030c393d (from 30a6f3c area; not pushed).

## Root cause
- Core `Shared::publish` (core/fsync-core/src/clip/send.rs) already fans out to peers concurrently (`join_all`), but returns only when ALL peers finished. Dead peer = 2 s first attempt + 5 s retry = ~7 s per call. Not serial per peer. Core has no cross-clip queue: reachable peers are not blocked inside the core.
- Apps run ONE publish at a time (Windows `_outLane` semaphore in ClipboardService.Outgoing.cs; macOS ClipboardSync `publishing`/`queued`; Linux calls the same `FsyncService::publish_clip`). Clip 2 waits for clip 1's publish call to return (slow peer) -> clip 2 late by ~5-7 s. Clip 1 reached reachable peers fast (646 ms), matching the evidence.
- Every clip repeated the full dead-peer cost, so every later clip was held back.

## Fix (core only, no C#/Swift change)
- New `clip/unreachable_peers.rs` (`UnreachablePeers`, 20 s back-off, mutex map, unit-tested), field `Shared.unreachable`.
- `send_one`: peer marked after a network failure (`CoreError::Net`) is skipped at once (reported `failed`); mark cleared on successful send, refusal/rejection (peer answered), or any inbound clip stream from that peer. After 20 s one publish probes again at full cost (one slow clip per 20 s while peer stays dead; peer is skipped, not retried, in between).
- Per-peer ordering unchanged (single publish at a time per app; dialer single-flight retained). No new tasks spawned, so nothing to leak on shutdown.

## Tests
- New `core/fsync-core/tests/clip_offline_peer_publish.rs`: 4 devices, one closed. First publish may pay full cost; next 3 publishes must return < 1 s, deliver to both live peers, and live peers must receive all 4 clips.
- Failed before fix ("clip 0: publish held up 7.0s"), passes after (7.1 s total, only first clip slow).
- `cargo test --locked --workspace`: all pass. `cargo clippy --locked -p fsync-core --all-targets`: clean. fmt clean.
- apps/linux workspace not in this worktree (branch predates it); not run. Only core internals changed, public API unchanged.

## Linux app (coordinator question)
Linux app (feat/linux-app) calls `FsyncService::publish_clip` -> `Engine::publish_clip` -> `Shared::publish` from the shared core (fsync-core path dep), so this fix covers it, assuming X2's 4132 ms Linux->Windows-CLI delay was an offline account device (matches: one failed peer = ~4-7 s). Could not verify against the JSON (not in this worktree). Note: if the 4.1 s was the first publish after the peer went offline, it is NOT removed (first clip still pays once); only following ones are.

## Commits
1. `test(core): offline peer must not delay following clip publishes`
2. `fix(core): skip recently unreachable peers so an offline device does not delay every clip publish`

## Unresolved
- First clip after a peer dies, and one clip per 20 s, still pay ~7 s. Removing that needs publish to return before slow peers finish (API change) or background probing.
- A peer that comes back online may miss clips for up to 20 s unless it sends us something first (clears mark). Presence hello not wired to clear the mark.
- Backoff 20 s chosen arbitrarily.
- ADR 014 open-issue entry not updated (file absent in this worktree).

**Status:** DONE_WITH_CONCERNS
**Summary:** Core fix covers Windows, macOS, Linux, CLI; test fails before / passes after, workspace tests and clippy pass.
**Concerns:** see Unresolved; Linux workspace and C# not run.

## Rework (after review 7.5/10)

Skip map (`unreachable_peers.rs`, `Shared.unreachable`, `receive.rs` clear) removed entirely (reverted in the fix commit). Reviewer's design implemented instead.

### Design
- `Shared::publish` (core/fsync-core/src/clip/send.rs): every peer send runs as an engine-owned task (`clip/background_sends.rs`, `JoinSet` in `Shared.background`). `publish` collects answers until all peers answered or `EngineConfig::publish_wait` (default `PUBLISH_WAIT` 1.5 s) passes; unanswered = `Published.pending`. Task does its own unoffer + debug log, so outcome is handled with nobody waiting.
- Shutdown: `Engine::close` calls `background.close()` (abort_all + join) before `node.close()`. After close, spawn refused -> peer reported `failed` (Closed). No task outlives close.
- Per-peer ordering (`clip/peer_send_lanes.rs`): before, ordering came from apps serializing publishes and publish awaiting acks. With background sends, clip N to a slow peer could land after N+1; receiver (`receive.rs`) has no ordering check and apps apply every RemoteClip -> stale clipboard. So: one lane per peer (async mutex held for the whole attempt) + sequence number taken in publish order; a send that gets its turn after a newer clip was queued for that peer is skipped (latest wins, unoffered). Ordering now same as before; at most one attempt per peer on the wire, so dead peer never piles up dials (dialer single-flight only dedups concurrent connects, the lane covers the rest; no extra state).
- Busy lane: a peer whose lane is still held by an older attempt (with serialized apps = an attempt that outlived its publish) is `pending` at once, not waited for. So in a burst of copies only the first pays up to 1.5 s, the next return as soon as live peers ack. Sparse copies (> ~7 s apart) with a dead peer each pay 1.5 s.
- Returning peer: nothing is marked; the next clip (or the queued latest) is tried for real -> gets it.
- Push (`opts.push`, send_files): no deadline, bypasses lanes (not superseded, does not supersede), waits for real result -> `PeerOffline` verdict unchanged.
- `PublishOpts.wait_all` (new) + `FsyncService::publish_clip_and_wait` (new, Rust only): `fsync-cli clip put` uses it (one-shot process stops the engine right after; would abort pending sends and retry-duplicate otherwise). CLI output/semantics unchanged (n7/n8 e2e expectations hold).

### API / consumers
- core `clip::Published` and `service::Published`: new `pending` field. FFI `Published` record unchanged (pending dropped; only plain comments added so Swift/C# bindings stay identical): pending peers are in neither `delivered` nor `failed`.
- Windows `ClipboardService.Outgoing.cs`: "N device(s) did not get it" uses `failed` only -> pending never shown as failure. Not changed. Caveat: if ALL peers are pending, `delivered=0` -> LastActivity "Copied, but no other device is connected" (status, not error).
- macOS `ClipboardSync.swift`: message only when `failed` non-empty -> fine, not changed.
- Linux `platform/src/sync/outbound.rs::describe`: new "Sending to N device(s)..." when nothing delivered yet but some pending (unit test added); fakes updated.
- fsync-cli: `publish_clip_and_wait`.
- Docs: ffi-contract.md, protocol-spec.md, system-architecture.md, project-changelog.md.

### Tests
- `tests/clip_offline_peer_publish.rs` (4 tests): every publish incl. first < 2 s with one dead peer, later ones < 1 s, dead peer `pending`, live peers get all 4 in order; returning peer (same identity, new engine) gets the next clip and never an older one after it; push to dead peer: `pending` empty, `failed=[peer]`, took > 1 s (full attempt, publish_wait 200 ms); close with pending send returns < 3 s (attempt had ~5 s left), publish after close does not panic.
- Unit: `background_sends` (close aborts + refuses), `peer_send_lanes` (supersede, one-at-a-time, busy, per-peer independence), Linux `describe`.
- Changed existing test `clip_refusal_feedback::rate_limited_offers_are_failed_not_refused`: it fired 60 publishes to ONE peer concurrently; with latest-wins lanes most became `pending`/superseded (1 delivered, 0 failed). Now back to back (as apps publish); still hits the rate limit (20 burst + 20/s) and asserts `pending` empty.
- Results: root `cargo test --workspace --locked` 375 passed / 0 failed / 3 ignored; clippy fsync-core 0 warnings; clippy workspace `-D warnings` clean; `cargo fmt --all --check` clean. apps/linux: `cargo build --locked --all-targets` OK (only the existing gold-linker warning); platform lib 125 pass, adversarial sync 7 + rules 7 pass. Real Wayland/X11 backend tests not run (no desktop touching). New offline test file stable over 6+ runs (~4.7 s).
- C#/Swift not changed, so nothing unverified there; bindings byte-identical (FFI record unchanged).

### Rework concerns / unresolved
- With a dead peer, a publish whose lane is free (first clip, or copies > ~7 s apart) still returns after 1.5 s; apps serialize, so the NEXT clip can start up to 1.5 s late. Bursts are not affected (busy lane -> no wait). Lower `PUBLISH_WAIT` would trade against slow-but-live relay peers being reported pending.
- Windows: when every peer is pending, status line says "Copied, but no other device is connected" (not an error). Fix needs `pending` in the FFI record (binding regen on Windows/macOS) or a C# heuristic; left alone.
- Engine close with a send to a dead peer still in flight: sends abort at once, but iroh `Endpoint::close` then waits ~3 s for the half-open connection (measured 3.0 s). Not a hang; service stop already bounds teardown.
- A pending peer's late outcome is only logged (debug); apps never learn it was delivered later. Same as before for failures (no retries).
- Concurrent publishes (not done by apps) report a superseded older clip as `failed` for that peer.

**Rework status:** DONE_WITH_CONCERNS (concerns above are observational).

## Fix round (after re-review 9.0/10)

Commits: 8ae0021 (L3), f59ee84 (M3 + L4/L5), 02c31de (M1 FFI + bindings), fe4a80c (M1/M2 apps), f820439 (docs, L6). Pushed to feat/linux-app.

### M1: `pending` in the FFI record
- `fsync_ffi::Published.pending: Vec<String>` appended LAST (same shape as delivered/failed/refused). Docs on the fields are real doc comments now (bindings regenerate anyway).
- Swift: `scripts/gen-bindings.sh` cannot run on Linux as is (hardcodes `libfsync_ffi.dylib`). Ran its exact steps with `libfsync_ffi.so` (same `uniffi-bindgen` bin, library mode): diff vs the checked-in file = only the `Published` hunks (no checksum or other drift), so the Linux run is equivalent. Copied `fsync_ffi.swift` only; header/modulemap go to untracked `build/`.
- C#: `uniffi-bindgen-cs` v0.10.0+v0.29.4 (commit e4f18be, the ADR 008 pin) built locally into the scratchpad, same args as `gen-bindings-cs.ps1` (`--library .so --config core/fsync-ffi/uniffi.toml --no-format`). Cross-check on waso-vm: `scripts/build-windows-dll.ps1 -Rids win-x64` (runs `gen-bindings-cs.ps1`) then `git diff --exit-code` on `Generated/` = identical. No hand edits.
- ffi-contract: record line, Semantics (pending = "not confirmed"; `delivered` empty + `pending` non-empty is not "no device connected"), "Additive since v2" entry (C# positional: regenerate + ship with the DLL).

### M1/M2: status wording (static, accurate without a later update)
| delivered / pending / failed | Linux | Windows LastActivity (LastError) | macOS message |
|---|---|---|---|
| 2/0/0 | Sent to 2 devices. | Sent to 2 devices | nil |
| 1/1/0 | Sent to 1 device; 1 not confirmed. | Sent to 1 device; 1 not confirmed | Sent to 1 device; 1 not confirmed. |
| 0/2/0 | Not confirmed by 2 devices. | Not confirmed by 2 devices | Not confirmed by 2 devices. |
| any/any/1 | ...; 1 did not get it. | (1 device(s) did not get it) — unchanged | Some devices couldn't be reached. — unchanged |
| 0/0/0 or only failed | Not sent: ... (unchanged) | Copied, but no other device is connected (unchanged) | failed msg or nil |
- Linux `describe` used to say "Sending to N device(s)..." (never resolved); now static. Linux also appends failed when something was delivered/pending (was silent).
- Windows: pure `ClipboardService.DescribePublished(delivered, pending, failed)`; app log line gains `pending=`. New `PublishStatusTextTests` (8 cases).
- macOS: `ClipboardSync.publishMessage(_:)` + XCTest `testPublishMessageCallsPendingDevicesNotConfirmedNeverUnreachable`. Uses `FsyncCoreKit.Published` (bare `Published` clashes with Combine). NOT compiled or run (no macOS here).
- No new event plumbing. ffi-contract states a pending peer's late outcome is only logged (debug).

### M3: latest-wins against a slow live peer
- New test hook `EngineConfig::clip_accept_delay` (`test-hooks` only, like `serve_delay`; release DLL check in build-windows-dll.ps1 still refuses test-hooks): receiver sleeps after reading an offer, before applying + acking.
- `slow_live_peer_gets_the_newest_clip_never_an_older_one_after_it`: b answers after 1 s (< 2 s first attempt, no retry), a's `publish_wait` 200 ms. Publish n (pending), n+1, n+2 (both pending at once: busy lane, not failed). b receives exactly `[n, n+2]`, then 2.5 s of silence. Mutation check: with the `latest == seq` check removed the test FAILS (`[n, n+1, n+2]`).
- `returning_peer_gets_the_next_clip`: after `next` arrives, keeps listening until the "while away" attempt surely ended (away publish + 8 s, at least 1 s), then asserts `next` is last and nothing else arrived.

### Lows
- L3: `background_sends` logs `warn` for a panicked send task (reap and close); unit test `a_panicking_send_is_reaped_and_does_not_poison_the_set`.
- L4: `QUICK` 2 s -> 3 s (dead peer still costs 7 s); busy-lane bound 1 s -> 1.25 s (still < 1.5 s `publish_wait`). Rate-limit test left as is (60 sequential publishes; not cheap to widen without losing the rate-limit hit).
- L5: close test uses `send_timeout` 30 s, so close < 10 s only if the send is aborted (measured 3.0 s; mutation without `abort_all` FAILS at 10 s). Publish after close now asserted: `Sent`, `failed == [b]`, delivered/pending empty.
- L6: protocol-spec + system-architecture: "2 s attempt, then 5 s retry (`send_timeout`), 7 s at most" instead of "5 s per-peer timeout".
- L1 (noted, not changed): concurrent publishes can interleave per-peer tickets; ordering holds only for serialized publishes (all apps serialize).
- L2 (noted, not changed): queued tasks behind a dead peer each hold their frame (<= ~256 KiB inline) up to ~7 s; a single latest slot per lane would avoid it.

### Verification
- Root: `cargo test --workspace --locked` 377 passed / 0 failed / 3 ignored; clippy `-D warnings` clean; `cargo fmt --all --check` clean. `clip_offline_peer_publish` 5/5 tests, 5 repeat runs (~8.1 s each).
- apps/linux: `cargo build --locked --all-targets` OK (existing gold-linker warning only); platform lib 125, adversarial sync 7, rules 7 pass. Wayland/X11 backend tests not run.
- Windows on waso-vm, ref f820439: DLL + bindings regenerated identical, `dotnet test Fsync.sln -c Release` 181/181 (CoreBindings 9, Interop 61, App 111).
  - Rig note: the orchestrator's new Rig 2.0.1 plugin `remote.cjs` reports "Rig worker runtime missing" on waso-vm (worker is Rig 1.1.0, protocol 1). The run used the pre-upgrade remote.cjs from `~/.claude/rig-backups/global/261110-154027` (doctor ok, same `remote run --ref` path as earlier runs). No permission flags, no allowedTools change, registry untouched. Command passed as `-EncodedCommand` (cmd mangled a quoted `-Command`).
- macOS: not built or tested (no macOS host). Swift binding is generated; app code + test unverified.

### Unresolved
- macOS compile/test of `publishMessage` and its test pending a macOS run (CI `macos` job will run on these paths).
- Rig version drift: upgrade waso-vm to Rig 2.x (plugin runtime) or keep using the 1.x script.
- `gen-bindings.sh` is macOS-only (`.dylib`); a `uname` switch to `.so` would let it run on Linux. Not changed.

**Fix round status:** DONE_WITH_CONCERNS (macOS unverified).
