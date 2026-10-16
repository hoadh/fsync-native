# fsync wire protocols (clip/2, file/1, pair/1)

Transport: iroh 1.x QUIC, one connection per peer pair per ALPN, cached by `Dialer` (retry once on a fresh connection). Accept gate: remote node id must be in the pinned, verified device list (except `fsync/pair/1` while one pairing window is open); untrusted live connections are closed with code 403 when trust changes. Types live in `core/fsync-proto`.

## Frames
`u32 LE length | postcard payload`. `MAX_FRAME_LEN = 1 MiB + 256 KiB`; length is checked before allocating. Raw data (file/1 CVs and bytes) follows frames unframed.

## ALPNs
| ALPN | Direction | Streams |
|---|---|---|
| `fsync/pair/1` | new device -> existing | bi, see pairing phase |
| `fsync/clip/2` | sender -> every online trusted peer | one bi stream per item: one `ClipOffer` frame out, one ack byte back (`1` accepted, `2` rate limited, `3` rejected) |
| `fsync/file/1` | receiver -> sender | one bi stream per fetch |

## clip/2 flow
clip/2 replaces clip/1 (uni stream, transport ack only), which could not tell a refusal from delivery (N8). Pre-release: no compatibility; a clip/1 peer fails the ALPN handshake at once, so a mixed-version pair fails fast instead of hanging.
1. Sender builds the offer: hashes every rep (blake3), inlines reps while the 256 KiB shared budget lasts, spills reps > 1 MiB to its blob store, snapshots files (size, mtime, blake3), `item_id` = UUIDv7. Total of rep sizes must be <= `max_clip_bytes` (50 MiB default) else `CoreError::TooLarge`.
2. Echo guard: if the content set hash equals what we last applied from a peer, nothing is sent (`PublishOutcome::Echo`); any other publish clears it.
3. Entry goes into the outbox (`offered_to` = target set) BEFORE sending. Sends run concurrently with a 5 s per-peer timeout; a peer counts as delivered only after its ack byte `1`. Failed and refused peers are removed from `offered_to`. A connection the peer closes with 403 (it removed or reset us; the accept gate closes after the handshake) is `refused` (`CoreError::PeerRefused`), distinct from offline; the cached connection is retried once on a fresh dial first. `Published.refused` lists such peers; single-target pushes (`send_files`) return `FsyncError::NotPaired`.
4. Receiver (per bi stream, 10 s read timeout; replies with the ack byte after processing): decode (no trailing bytes), `origin == connection peer`, `ClipOffer::validate` (version, inline hashes, budgets, names, `rel_path` safety), total (checked sum, overflow rejected) <= `max_clip_bytes`, per-peer rate limit (20 offers/s, burst 20; excess dropped before reading, answered with ack `2`), dedupe by `item_id` (ring of 1024), arm echo guard, history row (skipped when `ClipOffer.sensitive`: sensitive items are never stored, blobs fetched for them are removed after use), keep in the receive log (128 offers and 32 MiB of inline bytes/names/previews; oldest evicted together with their range readers), emit `EngineEvent::RemoteClip` (or `IncomingFiles` if `push` and kind Files). Any failure: `EngineEvent::Error` (no content in message), offer dropped.
5. Outbox lifetime: 1 h, or 10 min once a newer clip was published; swept on publish. Expired -> fetch answers `NotFound`.

## file/1 flow
`FetchReq{v,item_id,target:Rep(i)|File(id),offset (16 KiB aligned),len (0=to end),no_cvs}` -> `FetchResp{status: Ok|NotFound|Changed|Denied, size, hash}`.
After `Ok`: (unless `no_cvs` or <= 1 group) `ceil(size/16K)` 32-byte CVs, then data from `offset` in 1 MiB writes. Receiver: merged CV tree must equal offered hash and `FetchResp` must equal the offer (else `Integrity`); each group verified before it is written.
Sender policy: requester not in the trusted list -> `Denied` (checked first, so existence is not revealed); unknown/expired item or id -> `NotFound`; not in `offered_to` -> `Denied`; file size/mtime differs from the snapshot (or vanished) -> `Changed`.
Receiver size policy (before any transfer, `CoreError::TooLarge`): declared size <= free space of the blob volume (minus what a partial already holds), <= `EngineConfig.max_file_bytes` (default none), and its CV list <= 64 MiB (bounds files at 32 TiB); `fetch_file_to` also needs the size free at the destination. One fetch per blob hash runs at a time (single-flight). Each attempt has a deadline of 120 s + size / 256 KiB/s (`Timeout`, partial kept for resume). The sender abandons a stream whose peer does not read for 30 s.
Errors surface as `CoreError::Transfer(TransferError::{Integrity,Changed,Denied,NotFound,Timeout})`; a transfer with no progress for 30 s fails `Timeout`.
Blob store contract: `finish(hash, expected_size)` fails unless the `.part` exists and has exactly that length (never creates an empty blob); `put` writes through a unique temp file.
Resume: `<blobs>/<hash>.part` holds only verified bytes (written sequentially); a retry truncates to a 16 KiB multiple, re-verifies the prefix against fresh CVs, requests `offset = kept`. `FetchStats{size,resumed_from,bytes_received}`.
Ranged reads: `read_file_range(item,file,offset,len)` fetches 4 MiB windows (at most 8 readers cached, LRU, also dropped when their offer is evicted), prefetches the next window on sequential access, 30 s deadline (`read_timeout`), `read_file_range_blocking` for plain threads. Reads at/after EOF return empty.
Placement: `fetch_file_to(item,file,dest_dir)` re-validates `rel_path`, canonicalizes, checks containment before creating anything, creates directories component by component refusing symlinks, writes `<name>.<random>.fsync-part` (`create_new`, never follows links) and places it without clobbering: a taken name becomes `name (1).ext` and the final path is returned. Temp files are removed on error.

## pair/1 flow
Only reachable while a pairing window is open on the new device (N); the existing device (E) dials N. Frames: `Hello{v,mode,node_id,nonce_e}` (E) -> `HelloReply{nonce_n}` (N) -> [Code mode: `Spake` both ways] -> `Confirm{mac}` (E), `Confirm{mac}` (N) -> `DeviceInfo{name,platform}` (N, then E: who is being added / who adds you) -> `Accept` (N, after its user confirmed) -> `SignedList{chain}` (E) -> `Done` (N). `Abort{reason}` ends it at any step.
- Keys, MACs and the SAS derive from a transcript that includes both 16-byte nonces, so QR mode is fresh per session.
- Both devices show the same SAS words (`PairingEvent::ConfirmRequest`: on E for the new device, on N for the existing one, "Pair with <E name>?"). E publishes the new list only after its user agrees AND N sent `Accept`; N pins nothing before its own user agreed. A missing, negative or timed out decision aborts with nothing pinned on either side. Applies to Code and QR modes.
- Window: opened with a TTL. Up to 3 unauthenticated sessions run concurrently, one attempt per source node id; the first that passes the MAC claims the window (TTL stops, everyone else is refused). 3 MAC failures close it. `accept_bi` must deliver within 10 s. `await_pairing` reports the real outcome of the claiming session.

## Limits
| What | Value |
|---|---|
| Inline budget per offer | 256 KiB |
| Max clip (reps total) | 50 MiB (`max_clip_bytes`) |
| Spill threshold | 1 MiB |
| Frame | 1.25 MiB |
| Files / reps per offer | 10 000 / 16 |
| Chunk group | 16 KiB; CV overhead 0.2 % |
| Send timeout / read timeout / stall | 5 s / 30 s / 30 s |
| Outbox TTL / grace | 1 h / 10 min |
| Receive log | 128 offers / 32 MiB |
| Offer rate per peer | 20/s, burst 20 |
| Fetch deadline | 120 s + size / 256 KiB/s |
| CV list buffer | 64 MiB |
| Backend API | https only (`allow_insecure_http` for dev), responses <= 1 MiB, WebSocket frames <= 64 KiB |

## Measured (macOS arm64, single host, iroh over 127.0.0.1, release build, 2026-10-13)
| Test | Result |
|---|---|
| 1 GiB file/1 fetch (hash verified on every group, written to disk) | 8.32 s = 123 MiB/s (1.03 Gbit/s) |
| Sender offer: hash + CV list of 1 GiB | 0.79 s |
| 200 MiB fetch + export | 1.92 s = 104 MiB/s |
| Cold sequential ranged read, 40 MiB in 256 KiB reads | 352 ms (~114 MiB/s) |
| Inline text clip, publish -> `RemoteClip` event, warm connection | 58 us median, 69 us p95 (20 runs, bench); 0.24-1.1 ms in the in-suite test; 0.5-5 ms debug build |
| Resume after abort at 6 MiB of 64 MiB | resumed_from 6 291 456, bytes_received 60 817 408 (= size - resumed_from) |
Loopback only: no real link, no relay, first-connection dial time not included (connections are warm). Latency on LAN/relay will be dominated by RTT.
