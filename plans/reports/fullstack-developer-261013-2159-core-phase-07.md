# Phase 07 fsync-core: clip/1 + file/1 (public API delta)

All src files < 200 lines, no unwrap/expect in src. `cargo test -p fsync-core` (debug and release) green, clippy `-D warnings --all-targets` clean (core + proto), fmt clean, `cargo test -p fsync-proto` green. 1 GiB bench is `#[ignore]` (run once, numbers in docs/protocol-spec.md).

## Phase-06 follow-ups
- pair/1: initiator sends head only (`SignedList{chain:[head]}`); responder `TrustManager::pin_paired_head(head, me, peer, account)` (sig, shape, signer==peer, account, contains both) pins with `trust::UNKNOWN_ROOT`; chain path (`pin_paired_chain`) still accepted. `DeviceInfo.account_id: Option<Uuid>` added. `ingest_chain` learns the real root once a chain from genesis through the head is seen.
- `EngineEvent::SelfRemoved` (once) when a pinned list no longer contains our id; `TrustManager::set_self/is_self_removed`; `is_trusted` false afterwards. `NodeBuilder::spawn` calls `set_self`.

## New public API
- `Engine::start(NodeBuilder, Arc<dyn BlobStore>, EngineConfig)`, `node()`, `shutdown()`.
- `Engine::{publish_clip(ClipInput, PublishOpts) -> PublishOutcome{Echo|Sent(Published{item_id,delivered,failed})}, received_offer(&Uuid) -> Option<OfferRef>, fetch_rep(item, rep) -> Vec<u8>, fetch_file(item, file_id) -> FetchStats, fetch_file_to(item, file_id, dest_dir) -> PathBuf, fetch_blob_from(peer, item, FetchTarget, size, hash) -> FetchStats, read_file_range(item, file_id, offset, len) (async, cfg.read_timeout), read_file_range_blocking(..)}`.
- `EngineConfig{max_clip_bytes 50MiB, outbox_ttl 1h, outbox_grace 10min, spill_threshold 1MiB, send_timeout 5s, read_timeout 30s, serve_hook (doc-hidden, tests)}`.
- Inputs: `ClipInput::{text, files}`, `RepInput`, `FileInput::new(path)`, `PublishOpts{targets, push}`.
- Events: `RemoteClip{from, offer}`, `IncomingFiles{from, offer}`, `SelfRemoved`; `OfferRef` (Deref to `ClipOffer`, redacted Debug).
- Errors: `CoreError::{Transfer(TransferError), TooLarge{size,max}}`; `TransferError::{Integrity, Changed, Denied, NotFound, Timeout}`.
- `blobs::{BlobStore (sync trait: size_of, read_at, read_part_at, put, part_len, append_part, truncate_part, finish, remove, export), FsBlobStore::open(dir)}` (`<hash>` + `<hash>.part`, 0700; phase 08 wraps the trait for encryption).
- `net::Node::dialer()`, `net::Dialer`; `file::{fetch_to_store, FetchStats, Expect, ...}`.
- Proto (additive): `FetchReq{len, no_cvs}`, `FetchReq::ranged`, `file::CV_LEN`.

## Tests (new)
clip_protocol (6), clip_security (5: untrusted peer, not-offered Denied, malicious rel_path x6, foreign origin, oversized offer), file_transfer (7: 200 MiB hash, tamper, abort+resume, Changed x2, TTL NotFound, push), file_ranges (3 + ignored bench), group_tree (2), trust_head_pin_and_self_removal (2).

## Decisions / ADR
ADR 007 addendum: own file/1 with blake3::hazmat chunk-group CV outboard instead of iroh-blobs. Spec: docs/protocol-spec.md.

## Concerns
- Head-only pin leaves root unknown until a chain through the head is seen; replay of an old genesis then prompts a reset.
- Sender holds the CV list of each served item in memory (0.2 % of size) and re-sends it per fetch; resume is prefix-only (no sparse partials).
- `ListBackend::full_chain` unused by the initiator now.
- Spilled blobs are cleaned only on the next publish sweep; no progress events; no relay / two-machine run; encryption at rest deferred to phase 08.
- Two test files are ~205 lines (tests only).
