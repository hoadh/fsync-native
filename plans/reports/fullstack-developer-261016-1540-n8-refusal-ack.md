# N8 refusal ack (branch track-k/n8-refusal-ack)

Commits: 6f2402e (code, tests, n8 scenario), 1029f6a (docs, netns result json). Base: main 65b69a4.

## Design
- ALPN `fsync/clip/2` (was clip/1). One bi stream per offer: frame out, 1 ack byte back (`CLIP_ACK_ACCEPTED=1`, `RATE_LIMITED=2`, `REJECTED=3`, in fsync-proto). Receiver acks after `on_offer` (dup counts accepted); rate limit keeps `rx.stop` plus ack 2.
- Sender: delivered only on ack 1. Connection closed 403 (`conn.close_reason()` ApplicationClosed CLOSE_UNTRUSTED) -> new `CoreError::PeerRefused`. First attempt on cached conn is retried once on a fresh dial for Net or PeerRefused (cached conn may predate a re-pair).
- `clip::Published.refused` and `service::Published.refused`. Rate-limit/rejected stay in `failed` (rate-limit semantics unchanged).
- `send_files` -> `FsyncError::NotPaired` when refused (not PeerOffline). `clip put` bails NotPaired at once (no retry) when every non-delivered peer refused; JSON `published` gains `refused`.
- FFI: reused `NotPaired` (peer unpaired us = device must be re-paired; same app handling as local not-paired). No enum change, so no Windows C# change. FFI `Published` record unchanged: `refused` folded into `failed` (no app reads these lists); avoids Swift/C# regen. Say if you want a `refused` field exposed there (needs gen-bindings-cs on waso).
- Mixed version: clip/1 peer fails ALPN handshake immediately (test asserts fast failure), bounded by the attempt deadline anyway.
- CLI: only cmd_clip.rs touched. cmd_files.rs/ctx.rs untouched; `send` retries only PeerOffline, so NotPaired propagates and needs no CLI change.

## Tests
- New `core/fsync-core/tests/clip_refusal_feedback.rs`: warm conn then removal x5 tries, cold removal, clip/1 ALPN fails fast. Deterministic, no sleeps.
- Proof: temporarily restored old behavior (no ack wait, `tx.stopped()==None` => Ok): 2 of 3 tests FAILED with "reported delivered: Published{delivered:[A], refused:[]}". With the fix: pass.
- Raw-stream tests (clip_protocol, clip_security, security_limits) moved to open_bi.
- n8-reset-trust.sh: each of 3 puts and the file push must fail with "not paired with that device"; FAIL if delivered; FINDING removed; "did not come online" dropped.

## Gates
- cargo fmt --check: pass. clippy --workspace --all-targets -D warnings: pass.
- cargo test --workspace x2 (plus a third count run): 0 failures.
- p0 MIB=16: PASS (direct 16 MiB LAN, relay-only 16 MiB, clip 0.02 s).
- netns n8 (MIB=16, STRICT=1): PASS 21 s; notes: all 3 puts + file push refused, slowest put 5 s (its --wait settle), file push refused in 2 s. Run with the lead's uncommitted cmd_files.rs/ctx.rs copied in temporarily (needed so `send` has no wait_online gate), then restored via git checkout; they are NOT in my commits. Without the lead's change the file-push assert would see "did not come online". docker ps showed no fsync-netns containers beforehand.
- macOS: build-xcframework.sh ok (generated Swift identical, no diff), FsyncCoreKit swift test ok, FsyncMac swift build + swift test 0 failures.
- Windows: not touched (no FFI enum/record change). No Windows build needed.

## Notes
- docs/test-results/netns-261016-1557.json added (n8 run). Docs updated: protocol-spec, system-architecture, code-standards, project-changelog, test-results README.
- Plan file phase-03 todos not ticked (plan lives in main checkout).
- Not run: code-reviewer agent.

Unresolved: expose `refused` in FFI record? Plan phase-03 todo "review >= 9.5" pending.
