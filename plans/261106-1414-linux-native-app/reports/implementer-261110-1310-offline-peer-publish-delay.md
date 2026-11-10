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
