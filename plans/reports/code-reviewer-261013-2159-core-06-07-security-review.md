# Security review — fsync-core phases 06/07
Score 6.5/10 (8 after C1+H1-H3). Tests green. Crypto core (SPAKE2 roles/transcript, HKDF, ct MAC, head-pin rules, rollback, blake3 hazmat group verify) verified correct.

## Critical
- C1 New device (responder) pins dialer-signed head with no local SAS confirm (`pair/responder.rs:131-146`, `trust/ingest.rs pin_paired_head`). Backend knows code → dials N during window → passes SPAKE2 → N trusts attacker. Fix: N blocks on explicit PairDecision before pinning/Done; E→N frame with E's DeviceInfo; QR mode too.

## High
- H1 Concurrent same-hash fetches share `.part`; append check-then-act race; `finish` no length check, creates empty blob if `.part` missing (`blobs/fs_store.rs:82-107`, `file/resume.rs`). Fix: per-hash single-flight; `finish(hash, expected_size)`.
- H2 `export` temp `with_extension("fsync-part")` collides (a.txt/a.png), follows symlinks, rename clobbers; TOCTOU in `engine/place.rs:36-47`. Fix: random suffix + create_new, no-clobber placement, cleanup.
- H3 Unbounded CV list from peer-declared `FileMeta.size` → OOM (`file/fetch_client.rs:74-91`). Fix: size policy cap, free-disk check, overall deadline.

## Medium
M1 one dial burns pairing window; accept_bi no timeout (`net/pair_handler.rs:32`) · M2 pend_reset skips account check, unbounded pending/prompted (`trust/ingest.rs:98`, `trust/reset.rs`) · M3 `Shared.readers` never evicted (`engine/api.rs:154`) · M4 rep size sum overflow (`clip/receive.rs:84`), 160 MiB receive log, no per-peer rate · M5 create_dir_all before containment check (`engine/place.rs:36`) · M6 fsync under trust mutex (`trust/manager.rs`) · M7 await_pairing Timeout while session still pins (`net/node.rs:94`) · M8 QR mode deterministic, no nonces · M9 API client allows http, unbounded WS/response sizes, heartbeats stall.

## Low
NotFound before auth check (fetch_server:83) · write_all no timeout · `delivered` before ack · gate track/enforce race · dialer no single-flight · echo guard armed on receipt · no GC of received blobs/.part · fs_util temp collisions · serve_hook public · SAS string not zeroized.

## Decisions (lead)
1. Backend-issued code stays (plan); add mandatory N-side confirm + E DeviceInfo frame.
2. push offers: default ask (spec); receive cap = free disk space check + configurable max_file_bytes (default none) ; bounded CV memory.
3. H1/H2 fixed at BlobStore trait/contract level (encrypted store must honor).
4. Received blob GC → phase 08 retention.
5. Backend DoS in threat model → fix M2, M9.
