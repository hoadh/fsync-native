# Project Changelog

Newest first. Phase numbers refer to [the MVP plan](../plans/261013-2029-fsync-native-mvp/plan.md).

## 2026-10-14 - macOS Developer ID signing

### Added
- `build-fsync-mac-app.sh --sign IDENTITY --profile FILE [--notary-profile NAME]`: Developer ID signature with secure timestamp, embedded provisioning profile, and generated entitlements (`application-identifier`, `keychain-access-groups` = `TEAM.app.fsync.mac`). Checks that the profile matches the team and bundle id. `--notary-profile` notarizes, staples, and runs `spctl`.

### Verified
- Signed build (team VE5P5RS83W) launches with the profile accepted, uses the data-protection Keychain, and moves `history_key` + `device_secret` from the file keychain; the history DB opens with the moved key and a relaunch is stable. Notarized with `--notary-profile fsync-notary`: Accepted, stapled, `spctl` reports "Notarized Developer ID"; the notarized app launches.

## 2026-10-14 - macOS app shell and pairing (phase 11)

### Added
- `apps/macos/FsyncMac` SwiftPM app: menubar popover, main window, EngineBridge, Keychain secrets, email/OTP login, first device, pairing (offer code/QR, join, SAS), device list with status and remove, "Lost your other devices?" recovery, trust-reset prompt wiring, Settings (launch at login, sign out).
- `apps/macos/scripts/build-fsync-mac-app.sh`: release build into a signed (ad-hoc, hardened runtime) `FsyncMac.app`.
- `apps/macos/scripts/run-fsync-mac-integration.sh`: own Postgres container, dev backend, relay and `fsync-cli`; 3 integration tests (OTP login, CLI offers / Mac joins with matching SAS, online status, remove, Mac offers / CLI joins, `resetAccountTrust` recovery). Tests skip without `FSYNC_IT_API`.

### Fixed
- Keychain `set` failed with `errSecDuplicateItem` when the login model and the engine's session callback saved the session at the same time (sign-in then reported a storage error). Found by the integration run; covered by a concurrency test.
- Pairing code copy now marks the pasteboard `org.nspasteboard.ConcealedType` and `TransientType` so clipboard managers and future clipboard sync skip it.
- Device status: a peer online per the backend but without a connection no longer shows as "Offline" (core reports Lan/Relay only for live connections).

### Changed (review round 1 fixes)
- Boot and account-check tasks drop their results once cancelled, so sign-out during an offline check can no longer show a banner or move the phase.
- Keychain: duplicate probe counts as supported; data-protection `get` migrates items from the file keychain; `delete` clears both; calls are serialized (file-keychain add deadlock); `loadOrCreateKey` is add-only.
- Pairing: switching Code/QR keeps `.offering` so a SAS request is not dropped; a late `failed` event no longer replaces `done`/`failed`.
- One shared `SignOutButton` (confirmation, Cancel is default) in popover, main window and Settings.
- Main window is closed at launch so a returning user sees only the menubar icon.
- `selfRemoved` and the trust-reset outcome share `routeToPairing()` (dismisses pairing, clears devices).
- Engine errors are logged with `privacy: .private`; temp-dir fallback is 0700 with a banner; existing data dir is tightened to 0700; failed `saveSession` logs the server session out; popover says "Connected" only for a live LAN/relay link ("Online" for presence); dead "Recent items" placeholder removed; integration script stops the xctest child.
- `TrustResetPromptModel.outcome` is `internal(set)` so the app tests drive the real `$outcome` sink.
- Refreshed tokens are ignored while signed out (`SessionGate`).

### Known gaps
- Lan/Relay status is not exercised by the integration run (needs a connection; phase 12 sync).
- Manual checks (MenuBarExtra focus, confirmation dialogs, launch at login registration) are not recorded.
- After an offline sign-out the core keeps the old refresh token in memory until the next login.

## 2026-10-14 - macOS trust-reset prompt (SwiftUI library)

### Changed (review round 1)
- Consent binding: `confirm(resetId:)` / `decline(resetId:)` act on the id the user saw; the confirm dialog is bound to that id, closes when the shown reset changes, and lists the device names. Messages clear when the shown reset changes; a replaced reset shows "The device list changed. Review it again." `TrustResetOutcome.none` is now `.unanswered`. Long lists scroll; names wrap to 2 lines.

### Added
- `TrustResetPrompt` SwiftPM library (next to `FsyncCoreKit`, macOS 13): `TrustResetPromptModel` (refresh / handle event / confirm / decline off the main actor, `isBusy` double-tap guard, `NotFound` -> refresh + message), `TrustResetPromptView` over a stateless `TrustResetPromptContent`, outcome view. Device names render with `Text(verbatim:)` and are what the user checks (no reset code: the resetting device cannot show a matching one); trusting needs a second confirmation.
- `TrustResetPromptTests`: model against a real engine, `ImageRenderer` PNGs (`TRUST_RESET_SNAPSHOT_DIR`).

## 2026-10-14 - Trust reset review fixes (M5, round 3)

### Changed
- Pending resets: a new root replaces the pending one (an honest server serves one chain), so stale ids give `NotFound` and a new root is never dropped; the cap of 8 is gone. `confirm_reset` clears all pending entries (documented).
- The own-reset marker is a counter, so overlapping `reset_account_trust` calls do not end each other's protection. `is_own_fresh_root` also checks the account.
- `confirm_trust_reset`: a presence restart failure after a successful pin is logged, not returned.

### Tests
- Final-tree gate for M5 (fmt, clippy, workspace tests, Swift, e2e): see Verification status below.
- Wrong-account / multi-device fresh-root unit test; alternating resets by two devices do real PUTs until the 3/day limit (4th: 429 `Network`).

## 2026-10-14 - Trust reset API review fixes (M5, round 2)

### Changed
- FFI `contract_version()` is now 2 (`AccountEvent::TrustReset` fields, `FsyncError::ReauthRequired`).
- `TrustResetInfo` and `AccountEvent::TrustReset` carry `device_names`. `reset_id` is the hash of the offered head, not of the root: if the offered chain grows it is announced again and the old id gives `NotFound` (consent is bound to what was shown).
- `TrustManager::{confirm_reset, decline_reset}` take that head hash; `confirm_reset` returns `Ok(None)` for an unknown id, so a confirm / decline race is `NotFound`, not `Invalid`.
- Pending resets: cap 8 (replaced in round 3 by one pending root); declined roots: separate cap 32, only user answers add to it. The "prompted" set is gone (the pending entry records what was announced).
- `reset_account_trust` checks the local pin's account before the server call; if the server took the reset but pinning failed, the error says so and a retry pins the server's fresh root without a new reset. The initiator ignores resets of its own while it runs (`TrustManager::begin_own_reset`).
- `sync_device_list` skips the full-chain fetch while the server head is one the user declined. Note: a rollback-looking page (reset to a shorter chain) now reaches the reset path instead of being reported as "no change".
- `parse_reset_id` is a strict 64-hex decode (no trim); `is_self_removed` no longer reports true on a poisoned lock; undecodable pending entries are logged.

### Tests
- `service_trust_reset_edges.rs` (Ready branch, grown offer, pinned initiator, wrong account, retry without a new reset, confirm/decline race), `trust_reset_manager.rs`, `sync_device_list_reset.rs` (real backend), Swift test for the new fields. Pairing-free trust-reset tests run with `mdns: false`.

## 2026-10-14 - Trust reset API (review item M5)

### Added
- `FsyncService::reset_account_trust` (recovery with no device to pair with), `confirm_trust_reset`, `decline_trust_reset`, `pending_trust_resets`, `TrustResetInfo`; the same in the FFI. `TrustManager::{replace_with_genesis, decline_reset, pending_reset_infos}`.
- `FsyncError::ReauthRequired` (backend 401 `reauth_required`), also in the FFI error mapping and its roundtrip test.
- CLI: `account reset-trust --yes`, `trust resets [--json]`, `trust confirm <id>`, `trust decline <id>`. Account events are printed as structured JSON (`kind`, `reset_id`, ...), so `clip watch` shows `trust_reset` events.
- Tests: `core/fsync-core/tests/service_trust_reset.rs` (real backend: reset, confirm, decline, re-pair, stale auth) and a Swift test for the new surface.

### Changed
- `AccountEvent::TrustReset` carries `reset_id` (now the hash of the offered head, see the review fixes above) next to `devices`. FFI callers must update `switch` statements over `AccountEvent` and `FsyncError`.
- A confirmed reset that removes this device drops its pin so it can pair again (pairing refuses to run over a pin).
- A declined root is not offered again (neither announced nor listed) until the process restarts.

### Fixed
- `sync_device_list` never saw a reset to a chain shorter than the pinned one: the incremental page (`seq >= pinned seq`) was empty and read as "no change". An empty page now triggers the full-chain fetch, so old devices detect the reset.

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
- **Final tree (tester, 2026-10-14 11:48), all green:** `cargo fmt --check` PASS; `cargo clippy --workspace --all-targets -D warnings` PASS; `cargo test --workspace` 253 passed, 0 failed, 1 ignored (1 GiB benchmark, `#[ignore]` by design); `build-xcframework.sh` PASS; `swift test` 8/8; `tests/e2e/p0-exit-demo.sh` PASS (direct clip 0.03 s, relay-only clip 0.04 s; 256 MiB file sha256 verified both rounds).
- **Earlier interim run (01:30, superseded):** `cargo test --workspace` failed once on `devices_reset::trust_reset_without_fresh_auth_is_401_reauth_required` (testcontainers `PortNotExposed`). Not reproduced in the final run; root cause not verified.
- **Still open:** e2e has one recorded run on the final tree (target 3). Repeat runs are needed before a release.

### Known issues and open items
- Trust-reset pending/declined state is in memory only; a restarted app is prompted again after its next sync.
- Trust reset: the macOS app prompt UI is not built (phases 11/12), and there is no email notice on reset.
- Denylist setting is stored but not enforced in the core.
- Pairing: code mode relies on the mandatory SAS (the backend sees the code). A first connector can burn a window (DoS only). SAS is 36 bits.
- Presence and rate limits are in memory and per instance.
- No Postgres TLS (`tls-rustls` not enabled in sqlx).
- x86_64 xcframework slice is linked but has never run (no Intel machine).
- Windows: phase 03 C# half (bindgen-cs, DLL, callback stress) not done. Phases 04, 05, 13, 14 not started.
- Relay deploy (phase 10) not done. `deploy/relay/` is empty.
- Blobs in the data dir (including `<hash>.part` staging) are FSB1-encrypted. Files exported to user folders are plaintext by design.
