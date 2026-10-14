# Phase 07 — Core: clip/1 and file/1 protocols

## Context Links
- Design §4 flows 3-5; [phase-06](phase-06-core-identity-trust-transport.md); ADR 007 (iroh-blobs decision)

## Overview
- Priority: P0 | Status: Partial | Effort: 1.5-2 wk (A)
- Clipboard item broadcast to online paired peers; pull-based, blake3-verified, resumable file/rep fetch. Same transport for "Send to…" and copy-file-paste.

## Key Insights
- Single model: **sender announces, receiver pulls**. Small payloads inline (no RTT); big reps/files fetched on demand → enables lazy paste and resume.
- Echo loop: core tracks `last_applied_hash` per rep set; apps add origin marker type. Both checks.
- Sender must keep announced files readable: snapshot path + size + mtime; if changed on fetch → `Integrity` error (no silent wrong bytes).

## Requirements
- Clip latency LAN < 300 ms, relay < 1.5 s (inline path).
- File throughput LAN ≥ 80% link; resume after disconnect from last verified chunk.
- Limits: inline ≤ 256 KiB total; clip max 50 MiB (configurable); files unlimited in lazy mode.

## Architecture — frames (postcard, u32 length prefix)
```
ALPN fsync/clip/1  (sender opens uni stream per item to each online peer)
ClipOffer { v:1, item_id: Uuid(v7), origin: NodeId, ts_ms: u64,
  kind: Text|Rich|Image|Files,
  reps: Vec<RepMeta{ mime: String, size: u64, hash: [u8;32] }>,   // e.g. text/plain, text/html, text/rtf, image/png
  inline: Vec<Option<Vec<u8>>>,                                  // per rep, present if fits budget
  files: Vec<FileMeta{ file_id: u32, name: String, rel_path: String, size: u64, hash: [u8;32], mtime_ms: u64 }>,
  preview: Option<String /*≤ 200 chars for history*/> }
receiver: dedupe item_id; verify inline hashes; store history; delegate.on_remote_clip (auto mode → app writes clipboard)

ALPN fsync/file/1  (receiver opens bi stream per fetch)
FetchReq { v:1, item_id, target: Rep(idx) | File(file_id), offset: u64 }   // offset aligned to 16 KiB chunk group
FetchResp { status: Ok|NotFound|Changed|Denied, size: u64, hash: [u8;32] } then verified stream (bao/iroh-blobs ranges from offset)
receiver writes `<blobs>/<hash>.part`, verifies each chunk group, renames on complete; resume = re-request at verified len

Send to… (push intent, same pull)
FilesOffer over clip/1 with kind=Files + flag `push: true` → receiver auto-accept setting (default ask) → pulls into Downloads
```
Sender side keeps `OutboxEntry{item_id → reps bytes | file paths}` for TTL (default 1 h or until newer clip + 10 min) so lazy pastes resolve.

### Modules
```
core/fsync-core/src/
  clip/offer.rs, clip/send.rs, clip/receive.rs, clip/echo_guard.rs, clip/limits.rs
  file/fetch_server.rs, file/fetch_client.rs, file/resume.rs, file/outbox.rs, file/range_reader.rs  # read_file_range for Win IStream
  blobs/store.rs   # content-addressed dir, .part handling
```
Data flow: app publish_clip → limits+hash → outbox → ClipOffer to each Lan/Relay peer → peer receive → history+delegate → (paste) fetch_rep/fetch_file_to/read_file_range → file/1 → blobs store.

## Related Code Files
- Create: `core/fsync-proto/src/{clip.rs, file.rs}`, modules above
- Modify: `core/fsync-core/src/net/endpoint.rs` (register ALPNs)

## Implementation Steps
1. Frame types + codec + fuzz/proptest roundtrip.
2. clip send/receive + dedupe + echo guard.
3. file fetch server/client with verified streaming (iroh-blobs or bao-tree per ADR 007).
4. Resume + `Changed` detection; outbox TTL.
5. `read_file_range` (blocking, timeout 30 s, prefetch-ahead buffer for sequential reads).
6. Bench on LAN + relay; record.

## Todo List
- [x] Frames + codec tests
- [x] clip/1 send/receive
- [x] file/1 verified fetch + resume
- [x] outbox TTL + Changed handling
- [x] read_file_range
- [x] Benchmarks recorded in docs/protocol-spec.md

## Implementation Notes (sync 2026-10-14)
- Done: clip/1 inline + pull, file/1 with blake3 chunk-group CVs, resume, `Changed` and TTL `NotFound`, `read_file_range` (64 MiB clamp, LRU reader cache of 8), outbox, echo guard, dedupe, 20/s per-peer rate limit, 16-stream cap per connection, `max_file_bytes` + free-space check.
- Deviation: own file/1 instead of iroh-blobs (ADR 007 addendum). Added `push` and `sensitive` flags to `ClipOffer`. Sensitive items are not stored by the receiver.
- Concerns: sender keeps CV list in memory (~0.2% of size). Resume is prefix-only. Spilled blobs are cleaned only on next publish sweep. No progress events.
- Benchmarks (`docs/protocol-spec.md`, loopback): 1 GiB fetch 123 MiB/s, 200 MiB fetch+export 104 MiB/s, inline clip 58 us median.
- Read-ahead (core hardening, 2026-10-14, ADR 010): after two sequential reads `read_file_range` keeps 4 windows (16 MiB) in flight and drops windows behind the reader; random reads stay single-window. Review fixes: sequential means `offset == last_end` (a first read at offset 0 no longer counts as a run); prefetch draws from a shared per-peer budget (8 permits, `try_acquire`, never waits) so foreground reads keep stream headroom under the 16 streams/connection cap. Test `file_read_ahead.rs` (sender-side `serve_delay` test hook = one RTT per request; shell pattern: 256 KiB reads, one at a time, 200 MiB, loopback, release): before 0 ms 105.8 / 50 ms 45.8 / 100 ms 27.9 MiB/s; after 109.6 / 104.5 / 66.6 MiB/s (loopback hash/copy bound at ~105-110).
- Not done: proptest/fuzz roundtrip from step 1 (wire roundtrip tests only).
- Not measured: LAN throughput vs 80% of link, LAN clip <300 ms, relay <1.5 s, on two machines.

## Success Criteria
- Integration tests (in-process 2 endpoints): tampered chunk → Integrity error; kill mid-transfer → resume completes with correct hash; latency/throughput targets met on LAN bench.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Large clip broadcast to many peers saturates uplink | M x M | inline only small; rest pulled on demand |
| Blocking `read_file_range` stalls Explorer | M x H | prefetch, timeout, async capability (phase 05) |
| Outbox memory for big reps | M x M | spill reps > 1 MiB to blobs dir |

## Rollback
- ALPN versioning; revert crate commits. No persisted format besides blobs (content-addressed, safe to wipe).

## Security Considerations
- Only accept-gated peers; `FetchReq` authorized only if requester ∈ list and item was offered to it; path traversal: never accept paths from peers for sender reads (file_id lookup only); sanitize `rel_path` on receive.

## Next Steps
- Phase 08 exposes via FFI + CLI.
