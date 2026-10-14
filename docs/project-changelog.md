# Project Changelog

Newest first. Phase numbers refer to [the MVP plan](../plans/261013-2029-fsync-native-mvp/plan.md).

## 2026-10-14 - Track merge: core hardening, infra phase 10, macOS phases 04 and 12

### Added
- Core read-ahead in `read_file_range` (`file/range_reader.rs`, `file/seq_run.rs`): after two sequential reads in a row, up to 4 windows of 4 MiB are fetched ahead, with at most 8 windows held per reader. A first read is never sequential. Speculative fetches draw from a per-peer budget of 8 permits (`file/reader_cache.rs`) and never wait, so foreground reads do not queue behind them.
- Core exclusive data-dir lock: `start()` takes `<data_dir>/engine.lock` (std `File::try_lock`); `stop()` or dropping the service releases it. A second engine on the same directory fails with `Invalid("another fsync engine is already running on this data directory")`. The lock dies with the process, so a crash never blocks the directory.
- Core logout revocation (`service/revoke.rs`): see Changed.
- Infra phase 10 (`deploy/`, `.github/workflows/deploy-stack.yml`, `tests/e2e/compose-stack-sync.sh`): production compose stack (Postgres 17, backend, iroh relay) for one VPS behind the host Caddy. Backend and relay publish on loopback only (18080, 18081). Compose network pinned to `FSYNC_SUBNET` (default 172.29.0.0/24). `TRUST_PROXY_HOPS=1` with `TRUSTED_PROXIES` = that subnet + `127.0.0.0/8`. Scripts: `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `restore-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` (dedicated VPS only). See [deployment guide](./deployment-guide.md).
- CI: `ci.yml` path filters for the macOS and Windows jobs (always run on push to `main` and on dispatch). `deploy-stack.yml` builds the images and runs the compose stack end to end when deploy inputs change. Third-party actions are pinned by SHA.
- macOS phase 04 spike: `spikes/macos-clipboard` (self-test with a child reader process). Decision in [ADR 009](./adr/009-macos-lazy-paste.md).
- macOS phase 12 app (`apps/macos/FsyncMac`):
  - Clipboard sync: pasteboard watcher, sensitive filter, representation mapper (text, HTML, RTF, PNG), writer with echo guard, auto and manual modes, "Paste from <device>", `⌃⌥C` sends the clipboard.
  - History window: search, pin, delete, clear (pinned kept by default), paging of 50.
  - Settings tabs Sync, Privacy and Storage.
  - File clips: eager and lazy (`FileClipWriter`, `FileClipStrategy`), `FileFetchService`, `TransfersModel` (progress, cancel), `FilesSync` (incoming offers, Send to, drop on a device row), inbox retention.
- Tests: core `data_dir_lock.rs`, `logout_revocation.rs`, `file_read_ahead.rs`, a backend harness proxy (`backend_harness/proxy.rs`), and a logout test that revokes the whole token family (`backend/tests/auth_refresh.rs`).

### Changed
- Logout (core `service/login.rs`, `service/revoke.rs`): local sign-out runs first (tokens, pinned trust, presence) and `logout()` returns `Ok` after it, even when the network is down. The server-side revoke gets one inline attempt capped at 3 s. Transient failures (no answer, 408, 429, 5xx) are retried in the background, starting at 5 s and doubling to 5 min, for as long as the service lives. Other answers (401 = already revoked, other 4xx) end it. A refresh that was in flight cannot store tokens after logout. Core never persists the refresh token, so if the app quits first, the token stays valid until its 30-day expiry.
- Engine start and stop are serialized (`start_gate`), so concurrent `start` calls cannot race on the lock.
- `docs/ffi-contract.md` and `docs/api.md` describe the new logout, lock and `on_session_changed` behaviour.
- Backend test harness: when Docker never publishes the Postgres port (`PortNotExposed`), the container is removed and a new one started (4 attempts). Other errors fail fast. The core harness now shares this helper.
- Deploy layout: `deploy/` holds the production stack. `deploy/backend/compose.yml` is the older dev-only compose file. `deploy/relay/` is populated.
- macOS file clips: the default strategy is `hybrid`. Files under 20 MiB are fetched before the clip is written; larger ones are served on paste. Eager bytes per clip are capped at 512 MiB, and anything over the cap is lazy.

### Fixed
- Core: a file's first read is never treated as sequential, so a header read does not start prefetching.
- Core: a 401 from a refresh that started before a logout no longer clears tokens (epoch check in `ApiClient`).
- Deploy: a unique staging dir per deploy, a remote `flock`, atomic `.env` updates, `compose.yml.prev` kept for rollback, and a non-root relay (uid 10001) unless QAD needs the Caddy cert (`RELAY_USER=0`). Review fixes: `max_connections=30`, an OOM warning in smoke checks, readable non-secret files.
- macOS review round 1: publish queue, privacy recheck before send, settings load before edits, retention, file limits (commit 361568c). Round 2: the newest queued publish is kept, and a copy made during a publish is not lost.

### Verification
- Read-ahead, 200 MiB file, 256 KiB serial reads, in-process peers: 50 ms RTT 46 → 105 MiB/s. At 0 ms (loopback) there is no regression (about 106 → 110 MiB/s).
- Local e2e, 256 MiB: direct 35.6 MiB/s, relay-only 49.7 MiB/s.
- Public relay from Vietnam to the US-East-2 VPS: about 0.1 MiB/s, the same as the raw path. The path is network-bound; the relay is not the bottleneck.
- Infra: `tests/e2e/compose-stack-sync.sh` passed locally (compose up, HTTPS through a local Caddy, two CLIs, direct and relay clip, 4 MiB file over the relay, backup and restore). Shell and workflow lint are clean. The GitHub workflows have not run yet.
- Public HTTPS verified 2026-10-14 after the owner applied the Caddy snippet: smoke check, two-client e2e (login, pair with SAS, clip direct 22 ms and relay 802 ms, 16 MiB file direct and relay), and real client IP via `X-Forwarded-For` (spoofed header ignored; per-IP websocket cap enforced).
- macOS: integration run 12/12 against a real backend, relay and `fsync-cli`. Automated tests green. Manual QA not done.

### Known gaps
- Logout revocation is in memory only. A hard guarantee after an app quit needs an additive FFI (`revoke_session`); not done.
- Windows must re-run its 200 MiB shell paste against the new read-ahead (ADR 010 numbers were serial-bound).
- macOS manual QA open: the Finder, Mail, Slack and VS Code paste matrix for eager and lazy files, real password managers, and the privacy alert on macOS 15.x.
- macOS history keeps 200-character previews; copy-back works only for text stored in full. Full-content accessor not built.
- The denylist is applied by the macOS app to automatic sync (explicit send skips it, ADR 009). Core still does not enforce it.

## 2026-10-14 - Windows: C# bindings (03), clipboard spike (05), app shell (13)

### Added
- Phase 03 (C#): `core/fsync-ffi/uniffi.toml` (`[bindings.csharp]`: namespace `Fsync.Core`, public, `FsyncFfi`), `scripts/gen-bindings-cs.ps1` (checks uniffi-bindgen-cs v0.10.0+v0.29.4), `scripts/build-windows-dll.ps1` (DLL per RID into `Fsync.CoreBindings/runtimes/<rid>/native`). `apps/windows/src/Fsync.CoreBindings` (generated C#, `NativeLoader` picks the DLL for the process architecture) and `tests/Fsync.CoreBindings.Tests` (Swift tests ported + unstarted-engine errors). `spikes/ffi-bindings`: callback stress and buffer timing.
- Phase 05: `spikes/windows-clipboard` (listener, sensitive-format filter, `VirtualFileDataObject` with `IDataObjectAsyncCapability`, `RemoteFileStream`, multi-process self-test, WinUI 3 + H.NotifyIcon tray with a `--smoke` check and "Offer test files" menu). ADR 010.
- Phase 13: `apps/windows` solution. `Fsync.App.Core`: `EngineBridge` (account phase, background calls, UI-thread events), DPAPI `SecretStore`, view models (onboarding, pairing code/QR/SAS, devices, settings, tray status), `RunAtLogin`. `Fsync.App`: unpackaged self-contained WinUI 3 tray app, single instance per data dir, startup contract self-check. `tests/Fsync.App.Tests` (fake `IFsyncEngine`), `tests/e2e/pair-with-cli.ps1` (UI Automation against `fsync-cli` and a local backend).

### Fixed
- Core on Windows: `FsBlobStore::finish` opened `<hash>.part` read-only and called `sync_all`, which Windows refuses (FlushFileBuffers needs write access): every received or published blob failed with "Access is denied" (os error 5). Now opened for writing; handles are closed before renames (`finish`, `put`). Found by the app e2e; `blob_contract` reproduced it (3/6 failed on Windows).
- Accessibility: device list rows were announced as the record dump (`DeviceItem { NodeId = ... }`); `DeviceItem.ToString()` is now "name, platform, status".

### Decisions
- ADR 008: uniffi-bindgen-cs for Windows (no C ABI fallback). ADR 010: virtual-file lazy paste; clear our entry on exit (never `OleFlushClipboard`); core needs sequential read-ahead in `read_file_range` because the shell reads 256 KiB at a time, one read in flight.

### Verification (Windows 11 x64)
- C# contract tests 9/9 (real DLL); 10k callbacks from a Rust thread OK; 10 MiB round trip 27 ms (gate 20 ms missed, as on macOS). Phase 05 self-test 0 failures; tray smoke PASS. App tests 35/35; second instance redirects.
- App e2e (`apps/windows/tests/e2e/pair-with-cli.ps1`) PASS: OTP sign-in through the UI, code pairing with fsync-cli with matching SAS, devA shown online / offline from presence events without refresh, a devA clip delivered to the app's engine over QUIC, remove device, startup toggle. Rust workspace on Windows (first run there): 251 pass / 0 fail / 1 ignored; `cargo fmt --check`, clippy `-D warnings` (fsync-core) clean.
- Known gap: core reports Lan/Relay only while a connection exists and does not dial idle peers, so a paired, online device shows "Online, not connected yet" until traffic flows (the app publishes clips in phase 14).
- Not run: anything on ARM64 hardware (the ARM64 DLL and app build; clang 23 needed for `ring`) or Windows 10, pairing with a Mac.

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
