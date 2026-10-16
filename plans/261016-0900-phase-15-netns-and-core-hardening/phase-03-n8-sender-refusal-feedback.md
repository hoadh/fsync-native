# Phase 03: N8, tell the sender when the peer refused it (status: completed, merged 026fb5a; strict netns red from n3/n4 only)

Owner decision (2026-10-16): fix N8 now. The sender must learn that it was refused.

## Context
- Finding: plans/reports/fullstack-developer-261016-0900-track-n-netns-matrix.md (Round 2).
- Root cause:
  - `clip/send.rs` `attempt()` treats `tx.stopped() == None` (transport ack of the whole uni frame) as delivered.
  - The receiver's gate (`net/gate.rs` `Gated::accept`) closes an untrusted connection with 403 only after the handshake. By then QUIC has already acked the stream data.
  - Result: after A resets trust, B is told `delivered` in 2–3 of 3 tries. A stores nothing, so security holds; only the feedback is wrong.

## Requirements
- "Delivered" means the receiver accepted the offer at app level, not just at transport level.
- A refusal by a peer that no longer trusts us must be distinct from offline:
  - a typed error for single-target pushes (`send_files`, `clip put` to one node);
  - visible per peer in `Published` for broadcasts.
- Rate-limit refusal keeps its own meaning.
- No security regression: the gate still drops untrusted peers before any app processing.
- No silent hang on a mixed-version peer. Fail fast via the ALPN or a bounded timeout.

## Approach (dev may refine, and must justify)
- App-level accept ack on clip/1:
  - The sender writes the frame, then waits for a short reply from the receiver (e.g. a 1-byte status: accepted / rate-limited) before reporting delivered.
  - Connection closed with code 403 (`CLOSE_UNTRUSTED`) → refused.
- If the wire changes incompatibly, bump the ALPN (e.g. `fsync/clip/2`). Pre-release, so no back-compat shim (YAGNI).
- Extra latency is about 1 RTT, roughly equal to the current wait on `stopped()`.

## Tests
- A core test that reproduces N8 (A removes or resets B, B publishes): it must fail before the fix and pass after. Deterministic, no sleeps.
- Existing clip/file/rate-limit tests stay green.
- The netns n8 scenario asserts the refusal, and fails if `delivered` is reported.

## Todo
- [x] Ack protocol plus refusal classification in core (clip/2, app-level ack byte, `CoreError::PeerRefused`)
- [x] Error surfaced via `FsyncError` / `Published` (`NotPaired` for single-target; `refused` in core `Published`, folded into `failed` in FFI). FFI and Swift compile/test green (FsyncCoreKit 14/0, FsyncMac 231 run, 0 fail). No FFI enum change, so Windows build not required by this phase. Windows rebuild still needed for the clip/2 wire change
- [x] CLI output (`clip put` JSON gains `refused`) and n8 scenario updated (n8 PASS at MIB=64)
- [ ] cargo fmt / clippy / test ×2 and p0 MIB=16 done; netns strict still RED (n3/n4 quarantined, not n8)
- [x] Code review ≥9.5 (9.0, then 9.7)

## Results (merged 026fb5a)
- Fix: clip/2 with app-level ack. Sender reports delivered only on ack 1. 403 close (CLOSE_UNTRUSTED) -> `PeerRefused` -> `NotPaired` for `send_files` and single-target `clip put`. One retry on a fresh dial for Net or PeerRefused (a cached conn may predate a re-pair).
- Receiver: per-offer trust re-check after `accept_bi`, before rate limit, parse, or ack. Rate-limit stop: no redial.
- Tests: `clip_refusal_feedback` (4 tests) ~60 runs, 0 failures after review fix H1 (nondeterminism removed).
- Proof: old behavior (no ack wait) made 2 of 3 tests FAIL with "reported delivered".
- Scope changes: reused `NotPaired` (no new `Refused` variant, no FFI enum change). FFI `Published` folds `refused` into `failed`. ALPN bumped to clip/2. Pre-release, no clip/1 shim.
- Constraint: Mac and Windows apps must update together.
- Open: FFI `refused` field / dedicated `Refused` error for UI text. Mixed-version: `old_clip_alpn_peer_fails_fast_not_hangs` asserts a clip/1 dial fails fast (ALPN).
