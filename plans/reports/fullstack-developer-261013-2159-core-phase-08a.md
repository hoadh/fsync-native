# Phase 08 Part A fsync-core: history, encrypted blobs, retention, settings, SQLite trust

fmt clean, clippy `--workspace --all-targets -D warnings` clean, `cargo test --workspace` green. All src files < 200 lines, no unwrap/expect in src.

## Build decision
rusqlite **0.32** `bundled-sqlcipher-vendored-openssl` (macOS arm64 OK, FTS5 OK). 0.40 conflicts with sqlx 0.8 (`links = "sqlite3"`, libsqlite3-sys 0.30). XChaCha column fallback not needed. ADR 011 addendum written. New deps: `rusqlite 0.32`, `chacha20poly1305 0.10 (stream)`.

## Public API delta
- `history::History` (Clone): `open(dir, &[u8;32], Arc<dyn BlobStore>, &EventSink)`, `set_blob_guard(Arc<dyn Fn()->HashSet<Hash>>)`, `record(NewItem)->bool`, `list(after,limit)`, `search(q,after,limit)`, `item(id)`, `set_pinned`, `mark_rep_present`, `set_file_local_path`, `delete_item`, `clear(keep_pinned)`, `run_retention(now_ms)->RetentionReport{items_deleted,blobs_removed,bytes_freed}`, `blob_referenced`, `settings()`, `set_settings()`. Types: `NewItem::from_offer(offer,direction,peer,sensitive)`, item/rep/file/page cursor types in `history::types`. All async via spawn_blocking; one writer + one read-only connection; WAL.
- `settings::{Settings, ClipMode{Auto,Manual}}` defaults Auto / 30 d / 1 GiB / 50 MiB / [] / false; validated on set.
- `blobs::{EncryptedBlobStore::new(Arc<dyn BlobStore>, &[u8;32]), BlobInfo}`; `BlobStore` gains `list()` and `sweep_stale_parts(Duration)` (FsBlobStore implements).
- `trust::SqliteTrustStore::new(&History)`; FileTrustStore kept.
- `CoreError::Db(rusqlite::Error)`; `EngineEvent::HistoryReset{backup: PathBuf}`; `fs_util::ensure_private_dir` (0700).
- Engine: `Engine::start_with_history(builder, store, cfg, Option<History>)` (`start` = None). Records sent (before send) and received (before RemoteClip/IncomingFiles) items; retention at start + hourly; blob guard = outbox hashes. `ClipInput.sensitive` + `ClipInput::sensitive()` (published, never stored). `Outbox::blob_hashes()`; `clip::now_ms` pub.

## Behaviour
- Wrong key / not-a-DB / corrupt: `history.{db,-wal,-shm}` -> `history.corrupt-<ts>*`, fresh DB, `HistoryReset` event. Other errors propagate.
- Blob format: `FSB1` + 19 B nonce prefix + STREAM-BE32 64 KiB segments, XChaCha20-Poly1305, AAD = plaintext blake3, HKDF subkeys; random-access reads; truncation/tamper/wrong key fail. Partial downloads staged plaintext under keyed id, sealed on finish (bounded memory).
- Retention: age -> size cap (oldest unpinned) -> orphan GC (1 h grace, skips history/outbox-referenced) + stale `.part` sweep 24 h. Pinned never deleted.

## Tests added
history_db (7), history_retention (6), history_settings_trust (2), encrypted_blobs (6, incl. 64 MiB stream, widest inner IO <= 64 KiB+16), history_engine (4). Helpers in tests/history_common.

## Concerns
- Receiver cannot know an item is sensitive (no flag in ClipOffer; proto not ours): sender-side only.
- SqliteTrustStore is lost on history reset (key loss) -> re-pair.
- Plaintext `.part` staging during transfers; received files are plaintext by design; final blob filenames = content hash.
- Small race: delete-time GC vs concurrent record of identical content; mitigated by 1 h grace only.
- Never VACUUM (external-content FTS depends on rowid).
- `max_clip_bytes` not linked to `EngineConfig`, denylist not enforced (Part B/app).
- rusqlite pinned 0.32 (sqlx). Single reader connection. encrypted_blobs large test ~15 s debug. history_db.rs test file 199 lines.

## Unresolved questions
- Add a sensitive flag to ClipOffer (proto change) so receivers honor it?
- Verify SQLCipher vendored-openssl on Windows ARM64 / macOS universal in CI.
