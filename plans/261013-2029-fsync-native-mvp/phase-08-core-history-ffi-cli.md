# Phase 08 — Core: history store, retention, FFI surface, CLI harness

## Context Links
- Design §2 History, §4 flow 6; [phase-03](phase-03-spike-ffi-bindings.md) contract v0; [phase-07](phase-07-core-protocols-clip-file.md)

## Overview
- Priority: P0 | Status: Partial | Effort: 1.5-2 wk (A; +3-5 d history encryption; likely spills into P1 w1)
- SQLite history (local only), retention job, settings; implement FFI contract v0 for real; CLI harness used by tests + manual QA.

## Key Insights
- History = both sent and received items; content in blobs dir (content-addressed, dedup by hash).
- Retention must never delete blobs referenced by outbox or pinned items.
- CLI is the cross-OS test driver for phase 15 (headless, scriptable, JSON output).

## Requirements
- Search by preview text (FTS5), pin, delete item, clear all (keep pinned option), retention by days and total bytes.
<!-- Updated: Validation Session 1 - history encrypted at rest in MVP -->
- **Encryption at rest (MVP):** DB via SQLCipher (`rusqlite` feature `bundled-sqlcipher`), key = `EngineConfig.history_key` (32 B random, generated + stored by app in Keychain / DPAPI, same store as device key; CLI: file 0600). Clipboard blobs in `blobs/` encrypted XChaCha20-Poly1305 (per-blob random nonce, AAD = blob hash). Received *files* saved to user-chosen folders are NOT encrypted (user's files).
- Key loss (keystore wiped) → history unreadable → core detects decrypt failure, renames old DB to `history.corrupt-<ts>`, starts fresh; never crash.
- Migrations embedded (`rusqlite_migration` or sqlx sqlite) with version table.

## Architecture
### Schema (SQLite, WAL)
```
items(id TEXT PK, direction INT /*0 out,1 in*/, peer BLOB, kind INT, created_ms INT, total_size INT,
      preview TEXT, pinned INT DEFAULT 0)
reps(item_id TEXT, idx INT, mime TEXT, size INT, hash BLOB, present INT, PRIMARY KEY(item_id, idx))
files(item_id TEXT, file_id INT, name TEXT, rel_path TEXT, size INT, hash BLOB, local_path TEXT NULL, state INT)
items_fts USING fts5(preview, content='items', content_rowid='rowid')
trust(seq INT, hash BLOB, signed_bytes BLOB)   -- pinned device list (phase 06)
settings(key TEXT PK, value TEXT)
```
Retention: on start + hourly → delete unpinned items older than N days; then while blob bytes > cap delete oldest unpinned; GC blobs with refcount 0 and not in outbox.

### FFI + CLI
```
core/fsync-core/src/history/{db.rs, migrations.rs, queries.rs, retention.rs}, settings.rs, engine.rs (facade)
core/fsync-ffi/src/{lib.rs, engine_ffi.rs, types_ffi.rs, delegate_ffi.rs, errors_ffi.rs}
core/fsync-cli/src/{main.rs, cmd_login.rs, cmd_pair.rs, cmd_clip.rs, cmd_send.rs, cmd_watch.rs, cmd_bench.rs}
  fsync-cli --data <dir> login --email … | pair offer|join <code> | clip put --text … | clip watch --json
            send <node> <paths…> | devices --json | bench <node> --mib 1024 [--force-relay]
```
`--force-relay` (engine config flag, test-only) disables direct paths → relay test matrix.

## Related Code Files
- Create: files above; `core/fsync-ffi/uniffi.toml`; `docs/ffi-contract.md` (update v1)
- Modify: `scripts/build-xcframework.sh`, `scripts/build-windows-dll.ps1` (real crate)

## Implementation Steps
1. DB (SQLCipher keyed) + migrations + queries + tests (temp dir; wrong-key test).
1b. Blob store encrypt/decrypt (streamed for large reps) + tests.
2. Retention + blob GC + tests (pinned/outbox safety).
3. Engine facade wiring identity/trust/net/clip/file/history.
4. FFI adapters (thin, no logic) + regenerate bindings; publish xcframework/DLL artifacts from CI.
5. CLI commands; JSON output for scripting.
6. P0 exit demo: 2 machines (mac+win) CLI pair, clip, send 1 GiB LAN + relay.

## Todo List
- [x] SQLCipher keyed DB + blob encryption + key-loss recovery
- [x] SQLite schema + migrations
- [x] Queries + FTS
- [x] Retention + GC
- [x] Engine facade
- [ ] FFI adapters + CI artifacts
- [x] CLI harness
- [ ] P0 exit demo

## Implementation Notes (sync 2026-10-14)
- Done: SQLCipher history (rusqlite 0.32 `bundled-sqlcipher-vendored-openssl`, ADR 011), FSB1 XChaCha20-Poly1305 STREAM blobs, FTS5 search, retention (age, size cap, GC with history and outbox guards, 24 h stale-part sweep), settings, FFI contract v1 (UniFFI 0.29.4, ADR 008), `FsyncCoreKit` Swift package, JSON CLI, `tests/e2e/p0-exit-demo.sh`.
- Deviation: rusqlite 0.32, not 0.40 (sqlx `links` conflict).
- Deviation: wrong key no longer moves the DB aside. Keycheck tag gives `HistoryKeyMismatch` and nothing is moved. Explicit `reset_history(data_dir)` recovers (FFI free fn, CLI `history reset --yes`). A damaged file still auto-resets with `HistoryReset`.
- Deviation: partial downloads are staged as plaintext until `finish` seals them.
- Key storage: CLI uses a 0600 file. Keychain/DPAPI wiring is in phases 11/13.
- FFI: added `NotFound`, `HistoryKeyMismatch`, `pairing_cancel`. Removed test-only `diag_echo`.
- Open gate: Swift 10 MiB roundtrip was 37.8 ms in debug vs 20 ms target. Measured only in debug. Not re-measured in release, and the roundtrip test was replaced by a typed-error test, so the gate is not exercised in the suite.
- Done 2026-10-14: M5 TrustReset confirm API exposed in the FFI (`reset_account_trust`, `confirm_trust_reset`, `decline_trust_reset`, `pending_trust_resets`; contract v2) and CLI (`account reset-trust`, `trust resets|confirm|decline`). Trust lives in the history DB, so key loss also drops pinned trust. `logout` clears pinned trust.
- Not done under this todo set: macOS app prompt UI for the reset event (phase 11/12).
- Not done: CI does not publish bindings or xcframework artifacts (no such job in `ci.yml`). P0 exit demo ran on one Mac only (loopback, 256 MiB, not 1 GiB). The mac-to-Windows run is not done.
- Not verified: x86_64 xcframework slice is linked but never executed.

## Success Criteria
- P0 exit: CLI mac↔win pair/clip/send works LAN and `--force-relay`; CI publishes bindings artifacts; retention tests pass.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| FFI contract changes break apps | M x M | additive-only, CI builds both test apps against bindings |
| SQLCipher build on Win ARM64/macOS universal (OpenSSL/crypto backend) | M x M | use `bundled-sqlcipher-vendored-openssl`; verify in core-artifacts CI early; fallback: plain SQLite + encrypted `preview`/content columns via XChaCha |
| SQLite contention (watcher + UI queries) | L x M | WAL, single writer task, pooled readers |

## Rollback
- Schema migrations forward-only with tested down scripts for dev; app ships with core version pinned (xcframework/DLL embedded), so rollback = previous app release.

## Security Considerations
- History DB + clipboard blobs encrypted at rest (key in OS keystore, never on disk in plaintext on mac/win); zeroize key in memory on stop.
- data dir perms 0700 / user-only ACL; history excludes items flagged sensitive (never stored); clear-all securely unlinks blobs (best effort).

## Next Steps
- Unblocks 11, 13, 15 harness.
