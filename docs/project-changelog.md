# Project Changelog

Newest first. Phase numbers refer to [the MVP plan](../plans/261013-2029-fsync-native-mvp/plan.md).

## 2026-10-14 - P0 Rust track: core, FFI, CLI, backend

### Added
- **Phase 01 (scaffold and decisions):** Cargo workspace (`core/fsync-proto`, `core/fsync-core`, `core/fsync-ffi`, `core/fsync-cli`, `backend`), toolchain pin (Rust 1.99.0), ADRs 001-008, 011, 012. ADR 009 and 010 are not used.
- **Phase 02 (iroh spike):** single-host spike in `spikes/iroh-discovery` and ADR 007. Custom relay, path visibility, and blob transfer work on one Mac. Two-machine measurements (LAN mDNS, hole punch) are still open.
- **Phase 06 (identity, trust, transport, pairing):** device-list chain pinning (signed, domain-separated, seq and prev-hash checks), trust ingest and reset detection, accept gate (only pinned node ids), `fsync/pair/1` with SPAKE2 (code) or HKDF (QR), MAC confirm, 3-word SAS, pairing window, API client with single-flight token refresh, presence session.
- **Phase 07 (clip and file protocols):** `fsync/clip/1` (offers with inline and spilled reps, echo guard, per-peer rate limit, received-offer log) and `fsync/file/1` (own protocol: blake3 hazmat chunk-group CVs, resume from `.part`, ranged reads). Own protocol chosen over iroh-blobs (ADR 007 addendum).
- **Phase 08 part A (history):** SQLCipher history (`rusqlite` 0.32 with `bundled-sqlcipher-vendored-openssl`), `FSB1` encrypted blobs (XChaCha20-Poly1305), retention, settings, SQLite-backed trust store, ADR 011 addendum.
- **Phase 08 part B (facade, FFI, CLI, demo):** `FsyncService` facade, UniFFI 0.29.4 contract v1 (`docs/ffi-contract.md`, ADR 008), `scripts/gen-bindings.sh`, `scripts/build-xcframework.sh`, SwiftPM package `apps/macos/Packages/FsyncCoreKit`, `fsync-cli` (login, pair, devices, clip, send, fetch, history, bench), `tests/e2e/p0-exit-demo.sh`.
- **Phase 09 (backend):** email OTP auth with EdDSA access tokens and rotating refresh tokens, device-list registry with chain fetch, pairing codes, presence WebSocket, `deploy/backend` Dockerfile and compose file, `/v1` API (`docs/api.md`).
- **CI:** `.github/workflows/ci.yml` runs fmt, clippy (`-D warnings`), and `cargo test --workspace` on Ubuntu, macOS, and Windows. App jobs are placeholders.

### Security and review fixes

Each review round found issues; the fixes below are verified against code by the reviewers.

- **Core, phases 06-07 review:** pairing consent before pinning (both sides); nonces in the transcript; pairing admission limits (3 in flight, 3 MAC failures close the window); `BlobStore::finish` checks the expected length; export refuses symlinks and clobbering; file size caps (CV buffer 64 MiB, deadline, free-space check); reset requires the same account; reader LRU; receive budget and per-peer token bucket; serialized trust mutations; `ApiClient` refuses non-HTTPS unless a dev flag is set.
- **Backend, review rounds 1-2:** `TRUST_PROXY_HOPS` and `TRUSTED_PROXIES` replace the spoofable `X-Forwarded-For` trust; trust reset and account delete need fresh auth (5 min) and reset is capped at 3 per day; access tokens carry the session id and are bound to a live family; unauthenticated socket caps; header read timeout; heartbeat and hello rate limits; pending sockets expire; OTP resend and send-failure cleanup; shape and name checks; epoch-based history pruning; `earliest_seq` and `truncated` on the chain endpoint.
- **Core, phase 08B review rounds 3-7:**
  - Presence `hello` waits for the relay URL and re-sends on address change. This fixed a ~20 s relay-only delay (now about 0.03 s in the demo).
  - Pairing window opens synchronously with errors returned; `pairing_cancel` added.
  - Sensitive-item cleanup honours history and outbox references.
  - `FsyncService::run`/`start` return `Internal` inside a runtime, and drop is safe there.
  - WebSocket liveness: ping every 20 s, reconnect after 40 s idle.
  - Stream cap of 16 per connection.
  - History key mismatch is reported without moving files (`keycheck` tag). Only a damaged DB is moved aside.
  - Backup rules: newest 3 kept, data-bearing backups never pruned.
  - `send_one` waits for the peer's ack and uses one deadline per attempt.
  - Explicit `reset_history` (FFI and `fsync-cli history reset --yes`).
  - Re-pair after a history reset reuses the pinned head when the node id is already listed.
  - FFI `NotFound` and `HistoryKeyMismatch` error variants, with a roundtrip test.
  - CLI session files are cross-platform and written atomically with private permissions.

### Changed
- `ChainResponse` moved to `fsync_proto::api_dto` and gained `earliest_seq` and `truncated`.
- `ClipOffer.sensitive` added. Receivers record nothing for sensitive offers.
- Pairing frames carry 16-byte nonces. `PairingEvent::SasShown` replaced by `ConfirmRequest` on both sides.
- `BlobStore::finish(hash, expected_size)`; `export` returns the final path.
- FFI: `pairing_confirm(request_id, accept)` and `pairing_cancel`; `reset_history(data_dir)` added; `diag_echo` benchmark removed from the release surface.
- Backend: `TRUST_PROXY` replaced by `TRUST_PROXY_HOPS` plus `TRUSTED_PROXIES`.
- E2E demo asserts the `via` path and clip latency (`CLIP_MAX_S`, default 5 s); it makes `CARGO_TARGET_DIR` absolute before the spike build.

### Breaking or operational notes
- Backend migration `0002_refresh_session` adds session columns; sessions created before it have `auth_time = 0` and need a new login.
- Remove `TRUST_PROXY` from deploy configs. Use `TRUST_PROXY_HOPS` with `TRUSTED_PROXIES` when behind a proxy.
- If the history key does not match the DB, the core reports `HistoryKeyMismatch` and moves nothing. Use `history reset --yes` (or FFI `reset_history`) to start over.

### Verification status
- **Last recorded tester run (interim, 2026-10-14 01:30):** `cargo fmt --check` PASS; `cargo clippy -D warnings` PASS; `cargo test --workspace` FAIL on `fsync-backend` `devices_reset::trust_reset_without_fresh_auth_is_401_reauth_required` (testcontainers `PortNotExposed`). Likely a container-startup race, not verified. The run stopped there, so other crates and the e2e and Swift gates were not run in that session.
- **Implementer runs after that:** round 6 records `cargo test --workspace` with 56 ok result lines and 0 failed, Swift `swift test` 6/6, and one e2e PASS (direct clip 0.03 s, file 54.5 MiB/s send; relay-only clip 0.03 s, file 106.1 MiB/s send). Round 7 ran `cargo test -p fsync-core` only.
- **Not yet recorded:** one full gate run on the final tree. A rerun is required before a release.

### Known issues and open items
- `TrustReset` confirmation API (review item M5) is deferred. `logout` clears pinned trust.
- Denylist setting is stored but not enforced in the core.
- Pairing: code mode relies on the mandatory SAS (the backend sees the code). A first connector can burn a window (DoS only). SAS is 36 bits.
- Presence and rate limits are in memory and per instance.
- No Postgres TLS (`tls-rustls` not enabled in sqlx).
- x86_64 xcframework slice is linked but has never run (no Intel machine).
- Windows: phase 03 C# half (bindgen-cs, DLL, callback stress) not done. Phases 04, 05, 13, 14 not started.
- Relay deploy (phase 10) not done. `deploy/relay/` is empty.
- Blobs in the data dir (including `<hash>.part` staging) are FSB1-encrypted. Files exported to user folders are plaintext by design.
