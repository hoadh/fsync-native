# Core security fixes (phases 06/07 review)
Gates: `cargo test --workspace` (50 test binaries, 0 failed), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` clean; all src files < 200 lines.

## Per finding
- C1 pair/1: N blocks on explicit local `PairDecision` (same SAS) before pinning and before `Done`; E->N `DeviceInfo` (name, platform) after MAC confirm; N sends `Accept` only after consent, E publishes only after its own decision AND `Accept`. Reject/drop/timeout -> Abort, nothing pinned either side; Code + QR. `PairingEvent::ConfirmRequest` now emitted on N ("Pair with <E name>?"). Tests: `pairing_hardening` (N rejects Code/QR, attacker-with-code + N rejects, unanswered prompt), happy path both accept (`e2e_pair_flow`, `pairing_iroh`).
- M8: 16-byte nonces (`Hello.nonce`, `HelloReply.nonce`) in transcript. Test in `pair_kdf` (fresh keys/MAC/SAS per session in QR mode).
- M1/M7: `PairingWindows::admit` -> `PairTicket`; <= 3 concurrent sessions, one attempt per source node id, window consumed only by the session that passes MAC (`claim`), 3 MAC failures close it, 10 s `accept_bi` timeout, TTL stops once claimed, `await_pairing` returns the real outcome. Tests: failed guess doesn't burn window, 3 wrong guesses close it, `await_pairing` Timeout.
- H1: `BlobStore::finish(hash, expected_size)` errors unless `.part` exists with exactly that length (Fs + Encrypted; encrypted also serialises sealing); `put` via unique temp + rename; engine single-flight per hash (`file/locks.rs`). Tests: `blob_contract`, `encrypted_blobs`, `security_limits::concurrent_fetches_of_one_file_transfer_it_once`.
- H2/M5: export temp `<name>.<rand>.fsync-part` (`create_new`), no-clobber placement via `create_new` reservation + rename, `name (1).ext`, cleanup on error; dirs created component by component refusing symlinks after lexical containment check; `fs_util::write_private_atomic` uses random temp. Tests: `blob_contract` (symlink dest not followed, no clobber, no temp left), `file_transfer`.
- H3: `EngineConfig.max_file_bytes` (default None), `BlobStore::free_space`, `file/policy.rs` (`check_declared_size`, `check_fetch`, CV buffer cap 64 MiB -> `TooLarge`, per-attempt deadline 120 s + size/256 KiB/s on top of 30 s stall); dest dir free space checked in `fetch_file_to`; `read_file_range` honours the cap. Tests: `security_limits` (cap, tiny-disk store, policy bounds).
- M2: `pend_reset` requires same `account_id` (`WrongAccount`); pending/prompted keep newest 2. Tests in `trust_manager`.
- M3: `ReaderCache` LRU (8), evicted with their offer, prefetch task tracked + aborted (`RangeReader::shutdown`). Test: bounded reader count.
- M4: checked rep size sum; receive log budget 32 MiB (+128 offers) in `clip/received_log.rs`; per-peer token bucket 20/s burst 20 (`clip/rate.rs`). Tests: overflow rejected, flood throttled, log budget.
- M6: `TrustManager` mutations serialised by a write gate; `store.save` runs without the state mutex (persist, then swap).
- M9: `ApiClient::new` refuses non-https; `with_options(.., allow_insecure_http)` for dev/test (test harness uses it); WS max message/frame 64 KiB; presence loop uses `try_send` (full queue -> reconnect) so heartbeats never stall; response bodies capped at 1 MiB. Tests: scheme refusal, oversized response.
- Lows: auth before NotFound in `fetch_server`; 30 s write timeout on serve; dialer single-flight per peer+ALPN; `serve_hook` behind `cfg(any(test, feature = "test-hooks"))` (self dev-dependency enables it for tests); SAS is `Zeroizing<String>`; `fs_util` temp naming.
- Sensitive: `ClipOffer.sensitive`; receiver records nothing (history test asserts B has no row/bytes on disk); blobs fetched for sensitive items removed after `fetch_rep`/`fetch_file_to`.

## API delta
- proto: `PairFrame::Hello{+nonce}`, `HelloReply{nonce}`, `Accept`; `ClipOffer.sensitive: bool`.
- core pair: `transcript(.., nonces, ..)`, `PairKeys::sas() -> Zeroizing<String>`, `PairTicket`, `MAX_FAILED`, `PairingWindows::{admit, expire}` (replace `OpenWindow`/`take`); `PairingEvent::SasShown` removed, `ConfirmRequest` on both sides.
- blobs: `finish(hash, expected_size)`, `export -> Result<PathBuf>`, `free_space() -> Result<Option<u64>>` (default None).
- engine: `EngineConfig.max_file_bytes`, `fetch_file_to` may return a renamed path, `Engine::open_readers()` (hidden), new `file::{check_fetch, check_declared_size, fetch_deadline, max_verifiable_size, MAX_READERS}`, `clip::{ReceivedLog, ReceivedOffer, KEEP_BYTES, checked_total}` public.
- api: `ApiClient::with_options(base, tokens, cb, allow_insecure_http)`; `new` is https-only.
- Cargo: dep `fs4`; feature `test-hooks`.
- docs/protocol-spec.md updated (pair/1 section, limits, file/clip policy).

## Concerns
- Pre-MAC slots: attackers with many node ids can stall <= 3 slots until step timeouts (DoS only); code-guess probability <= 3 per window.
- A wrong-code single attempt no longer burns the window (new side just times out at TTL).
- Encrypted store seal mutex can briefly block other spills during a large `finish`.
- `RangeReader` still holds `Arc<Shared>` (cycle broken only on eviction); pre-existing.
- Received-blob GC remains phase 08 (decision 4).
