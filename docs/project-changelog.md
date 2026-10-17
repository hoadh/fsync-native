# Project Changelog

Newest first. Phase numbers refer to [the MVP plan](../plans/261013-2029-fsync-native-mvp/plan.md).

## 2026-10-17 - Peer refusal is its own error in the FFI and the apps

### Changed
- FFI: new `FsyncError::PeerRefused` (appended last, index 15) for a single-target push (`send_files`) to a peer that removed or reset this device. `NotPaired` now means only "no such paired device here". `Published` gains `refused: [node]`. Supersedes the 2026-10-16 line "No FFI change: ... folds `refused` into `failed`; the error reuses `NotPaired`". See [FFI contract](./ffi-contract.md).
- CLI: `clip put` fails with `PeerRefused` when every peer refused; `send` shows the same error.
- macOS and Windows: new text "That device removed this Mac/PC. Pair again to send to it." instead of "Something went wrong (not-paired)" (macOS) and "That device is not paired." (Windows).

### Upgrade
- Hosts regenerate the Swift and C# bindings. Swift `switch` statements over `FsyncError` need a `PeerRefused` case.

## 2026-10-16 - Core hardening: offline start, fetch retry, redial stages; CLI path tier; netns matrix

### Added
- Core: offline start. When the backend is unreachable, a device whose pinned list includes itself keeps working from that list, for up to 7 days after its last device-list sync (`core/list-synced-at`; installs with no stamp age from the `history.db` mtime). Only connect, timeout and DNS errors qualify. TLS, bad bodies and every HTTP status (502, 503 and 504 included) still fail. See [system architecture, section 9](./system-architecture.md#9-offline-start-cached-device-list).
- CLI: `path` on `sent` and `devices` (`lan`, `direct`, `relay`, or null); `path_tier` on `fetched` (`path` there is the saved file). `RUST_LOG` turns on traces on stderr.
- Test harness: netns matrix `tests/e2e/netns` (N1-N9 in network namespaces, privileged Docker), run with `run-matrix.sh --scenarios netns`. `NETNS_STRICT=1` fails on a gating quarantine. `NETNS_MAX_S` bounds the run. See [test results](./test-results/README.md).

### Changed
- CLI `send` no longer waits for presence: the offer dials the peer. It retries only while the peer is offline, within `--wait`. After the offer, the peer must start pulling within max(`--wait`, 10 s), or `send` fails with "did not start fetching the offer".
- Core redial: stalled dials clean up in stages (own stale outbound, then idle inbound, then in-use clip and presence inbound). Running file pulls are always spared. See [architecture, section 8](./system-architecture.md#8-redial-after-a-peer-restarts).

### Fixed
- Core: a file fetch on a cached connection could hang after the peer restarted. The first attempt now has 5 s, then one retry on a fresh dial. Protocol answers are not retried.
- Core: the clip receiver re-checks trust for every offer, so a removed peer is closed with 403 before processing, never acked.
- Core: a rate-limited offer no longer ends the connection. Its stream is stopped, it gets ack 2, and the connection stays open.
- Core: starting while our own previous engine still holds `engine.lock` logs a warning with the remaining hold.

### Upgrade
- No FFI change in this batch. `ensure_account` may now return `Ready` offline (see [FFI contract](./ffi-contract.md)).

### Verification
- Final merged tree at `026fb5a` (tester, 2026-10-16): fmt and clippy `-D warnings` clean; `cargo test --workspace` 354 passed, 0 failed (two runs); xcframework builds; `swift test` FsyncCoreKit 14 and FsyncMac 231 executed, 18 skipped (integration, needs `FSYNC_IT_API`), 0 failures; `p0` at 16 MiB PASS.
- Netns 64 MiB: PASS 8, QUARANTINED 2 (n3, n4), SKIP 1, on both strict and non-strict runs. N2 and N9 (backend down) now PASS. n3 and n4 are quarantined for lack of a direct path and a fast first connect, see the results page.

## 2026-10-16 - N8: senders learn when a peer refused them

### Fixed
- Core: after a peer removed or reset a device, the sender was told "delivered" because clip/1 only had a transport ack (the accept gate closes with 403 after the handshake). Clip offers now use `fsync/clip/2` (bi stream, the receiver answers with an app-level ack byte). "Delivered" means accepted. A 403 close is `Published.refused` (`CoreError::PeerRefused`); `send_files` and `clip put` returned `NotPaired` (superseded 2026-10-17: now `PeerRefused`), not `PeerOffline`. Rate-limit and invalid-offer replies stay in `failed`.
- Wire: ALPN bumped, no compatibility. A clip/1 peer fails the handshake at once, never hangs. No FFI change: the app `Published` record folds `refused` into `failed`; the error reuses `NotPaired`.

### Upgrade
- The macOS and Windows apps must be updated together: clip/1 and clip/2 do not interoperate, so a device on the old version cannot send or receive clips with one on the new version.

### Verification
- `core/fsync-core/tests/clip_refusal_feedback.rs` fails with the old transport-ack behavior and passes now. The netns N8 scenario asserts the refusal on every try.

## 2026-10-15 - LAN-first path selection and macOS history device names

### Added
- Core: LAN-first iroh path selection (`net/lan_first_path_selector.rs`, `net/local_subnets.rs`, `net/subnet_cache.rs`, installed in `node_builder.rs`). Tiers: LAN, then other direct (VPN or hole punch), then relay. RTT orders paths only inside a tier. The LAN test is generic (on-link subnets of physical, up, gatewayed interfaces); no vendor ranges. See [system architecture](./system-architecture.md#path-selection-lan-first).
- Deps: `netdev` 0.46 (`gateway` feature).
- macOS: history rows show the sender's device name ("another device" when unknown), matching Windows.

### Changed
- iroh is pinned at `=1.3.0` with `unstable-custom-transports`. Re-check the selector on upgrade.

### Verification
- Same-LAN Tailscale runs were 34–56% slower than plain LAN ([throughput report](../plans/reports/tester-261015-2130-tailscale-vs-lan-throughput.md)). Live check: 256 MiB each way over LAN with Tailscale up, 58 and 64 MiB/s.
- Core unit tests cover tier ordering, switch margin, IPv6 bias, subnet classification and cache refresh.

## 2026-10-15 - macOS app fixes: first launch, This Mac status, popover anchor, history refresh

### Fixed
- macOS: the app window stayed on "Starting fsync…" and Sign In did nothing at first launch. `MenuBarExtra`'s `.onReceive(bridge.$phase)` fires in `willSet`, so the window opened with the stale phase. Opening is now deferred one main-queue turn (`FsyncMacApp.swift`, commit `4111ee2`).
- macOS: the "This Mac" row showed "Offline" in the main window and the menubar popover. The core has no connection to itself, so the row now hides status for the self device (matches Windows "This PC").
- macOS: the menubar popover drifted below its icon after its content shrank. The `MenuBarExtra` window resizes keeping its bottom-left origin; new `MenuWindowTopAnchor` keeps the top edge fixed.
- macOS: the clipboard history window stayed empty or stale while Windows showed items. History loaded once at start and SwiftUI kept the closed window's view alive. It now refreshes on received and sent clips/files and when the window opens. Refreshes in one turn are coalesced and keep the rows already loaded (the engine caps a read at 200 rows).

### Verification
- macOS: Core `swift test` 228 run, 18 skipped, 0 failures; UI 9/9. 9 new regression tests (including `EngineBridgeHistoryRefreshTests` and `MenuWindowTopAnchorTests`).

## 2026-10-15 - Hardening round 2: redial, copy-back accessors, health probe, off-box backups, rollback guard

Four tracks (core, macOS, Windows, infra) merged into local `main` at `3c405b1`. Round-2 reports: `plans/reports/fullstack-developer-261015-0425-core-round-2.md`, `-0330-windows-round-2.md`, `-0301-macos-round-2.md`, `-0450-infra-round-2.md`.

### Added
- FFI (contract stays 2, additive): `FsyncEngine.history_item_reps(item_id)` (Swift `historyItemReps(itemId:)`) returns the full content of a history item for copy-back. `FsyncError::EngineAlreadyRunning` is a typed unit variant (was `Invalid` text). See [FFI contract](./ffi-contract.md#additive-since-v2-no-version-bump).
- macOS: full-content copy-back from History (`ClipboardSync.copyBack`, `HistoryItem.canCopyBack`); typed engine-lock message.
- Windows: History Copy loads full content (eager up to 256 KB, lazy above; unconvertible reps skipped); lock error caught by type, string matching removed.
- Backend: `GET /health/otp` (200 `ok`, 503 `otp cap reached`). See [api](./api.md#auth).
- Deploy: `deploy/scripts/probe-health.sh` and hourly `.github/workflows/health-probe.yml`; `deploy/scripts/rollback-guard.sh` with `deploy.sh --force-rollback`; nightly encrypted off-box backups (Ohio to Singapore, upload-only, 30-day retention) with a restore drill; `tests/e2e/run-matrix.sh`. Runbook: [deployment guide](./deployment-guide.md), [deploy/operations.md](../deploy/operations.md).
- Tests: core `dial_after_restart.rs`; macOS hermetic `CliProcessTests`, `RealBackendRevokeTests` (gated), `ClipboardStartAfterFailedLoadTests`; Windows `CancelStartedTransferTests`, `FailedSaveLeavesNoPendingRevokeInMemory`.

### Changed
- Core redial: after a peer restarts on a new port (same node id), the redial takes about 2 s instead of 15-30 s. The first dial attempt has a 2 s budget; on stall, stale outbound same-ALPN connections are closed and the dial retries with current addresses. See [system architecture](./system-architecture.md#8-redial-after-a-peer-restarts).
- Relay failover is documented as verified for connect time only. Mid-run relay death is unverified, and relays do not mesh (peers on disjoint relays cannot connect).
- Windows: the pending revoke is kept in memory only after a successful save. `Dispose` cancels the ticket before closing the gate.
- Backend test harness: Mailpit HTTP calls retry on transient resets (`backend/tests/common/mailpit.rs`); Postgres start, admin connect and service calls have timeouts.
- macOS CI: `CliProcess` reports exit only after both pipes drain (1 s grace, then force-drain); no `super` calls in `IntegrationTestCase`.
- Ohio host redeployed from main: release `f00980db3978` (rollback target `549cc6a3d298`); public smoke and probe-health ok.

### Fixed
- Core: a crashed peer that returns on a new port is reachable in about 2 s (above).
- macOS: sync stays off after a failed initial settings load (`caed6cc`). Swift 6.1 CI compile errors fixed (`CliEvent` is Sendable).
- Windows: `Dispose` cancels the in-flight write before closing the gate; `WriteAsync` is a no-op after dispose.
- Infra: CI Rust job no longer dies with `ld` SIGBUS (debuginfo off, free-disk step in `ci.yml`).

### Incident: rollback rehearsal on the Ohio host (2026-10-15)
- While rehearsing `deploy.sh --rollback`, the rollback target `d9e5e9b7ef77` predated migration 0003. The database was already at 3, so the backend crash-looped (sqlx: "migration 3 was previously applied but is missing").
- Impact: `api-fsync.10z.one` returned 502 for about 5 minutes (about 03:08-03:13, +07) until the owner rolled forward. The macOS app saw the same 502 and kept its pending revoke, as designed.
- Fix: `rollback-guard.sh` refuses targets whose migrations lack the database's max version (verified against `d9e5e9b7ef77`). `--force-rollback` overrides it.
- The outage came from the rehearsal itself, not from a bug in a shipped release. Data was not touched by the rollback (images and compose file only).

### Verification
- Core track: `cargo test --workspace` 307 passed, 0 failed, 2 ignored; fmt and clippy `-D warnings` clean; build-xcframework OK; reviewer 9.5/10.
- Infra track: `cargo test --workspace` 301 passed, 0 failed; shellcheck clean; reviewer 9.5/10. The health-probe workflow can be verified only after the merge and once Actions is unblocked.
- Backups: shipments and prune test on the Singapore host; restore drill on a Mac in throwaway `postgres:17` containers (decrypt, `pg_restore`, row counts match live: accounts 1, device_lists 1, history 2, refresh_tokens 2, migrations 3).
- Rollback: a compatible pair (`519ec821e90f` <-> `549cc6a3d298`) rolled back and forward, about 88 s per hop, one failed 0.5 s poll per hop.
- E2E matrix: `MIB=8 p0 local,sg` both PASS (local 239 s; sg 51 s, about 0.38 MiB/s; clip latency 0.53 s).
- macOS: `swift test` (FsyncMac) 222 run, 18 skipped, 0 failures; `FsyncCoreKit` green; `mac-it` PASS.
- Windows: `dotnet test` 155/155 (9 + 47 + 99 across the three test projects); new tests 10x green. After B5: 156/156.

### Open
- **GitHub Actions is blocked by billing.** No CI, deploy-stack or health-probe run has started since the block. The last green macOS run predates later changes. Re-dispatch `ci.yml` once unblocked. Until then the hourly probe does not alert.
- Windows CI dispatch `38635166526` failed in `EagerBudgetCoversStartingTheTransfers` (before the round-2 fixes). Not re-run in CI.
- **DKIM is missing for `10z.one`.** No DKIM record at common selectors (checked read-only with `dig`). Mail passes DMARC via SPF alignment only, and DMARC is `p=none`. Owner: enable DKIM in Google Admin, publish the `google._domainkey` TXT, review DMARC reports, then tighten the policy.
- Backup failures are visible only in `backup.log` unless `BACKUP_PING_URL` is set. Owner decision: set it or accept log-only. The backup private key must be in the owner's password manager; losing it makes off-box copies unreadable.
- Probe cadence: hourly runs use about 720 of 2000 free private-repo minutes a month.
- Core: mid-run relay failover unverified; file and rep fetches do not retry on a dead cached connection.
- macOS live-token revoke run (`RealBackendRevokeTests` with `FSYNC_REAL_REFRESH`) needs an owner refresh token; the local-backend proof stands meanwhile. Manual GUI QA for macOS and Windows is still open.
- Hard logout: a host never launched again keeps its refresh token until expiry (30 days). Unchanged.
- Redeploy the Ohio host from `main` after the merge.

## 2026-10-15 - OTP email over SMTP, relay-only deploy, Singapore relay, Windows phase 14, multiple relays, hard logout

### Added
- OTP email sender `EMAIL_SENDER=smtp` (lettre, STARTTLS on `smtp.gmail.com:587` by default; 465 = implicit TLS). Env: `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`, optional `SMTP_HOST`, `SMTP_PORT`. Production refuses `log` unless `ALLOW_LOG_EMAIL=1`. Code: `backend/src/email/` (`smtp.rs`, `config.rs`, `sender.rs`).
- Global OTP send cap `OTP_DAILY_SEND_CAP` (default 450 per rolling 24 h), counted in Postgres (migration `0003_otp_send_log`) under an advisory lock. Over the cap, `otp/start` returns 429 and logs `global OTP send cap reached`.
- Relay-only deploy mode: `deploy/compose.relay-only.yml`, `deploy/relay-only.env.example`, `deploy.sh --relay-only`, and a `.mode` marker on each host. `smoke-check.sh --relay-only`, `test-relay-only-local.sh`.
- Singapore relay `relay-sg-fsync.10z.one` on a relay-only host (release `519ec821e90f`). See [deployment guide](./deployment-guide.md#hosts).
- Windows phase 14 merged: clipboard sync, files, history and settings. Default hotkeys Win+Alt+C (send) and Win+Alt+V (paste from another device). Windows 11 x64: `dotnet` 124/124.
- `tests/e2e/p0-exit-demo.sh` accepts `E2E_RELAY_URL` to run against a deployed relay.
- Core multiple relays: `EngineConfig.relay_urls` (last field, uniffi default `[]`), merged with `relay_url` (first), duplicates dropped. All relays go into one iroh `RelayMap`; the endpoint picks the home relay by latency. Code: `core/fsync-core/src/net/relays.rs`, `core/fsync-ffi/src/engine.rs`.
- Core and FFI `logout_confirmed()` (`true` = backend settled the revoke) and free function `revoke_session(api_url, refresh_token, allow_insecure_http)`, which needs no engine. A 401 counts as success. See [FFI contract](./ffi-contract.md#additive-since-v2-no-version-bump).
- macOS pending revoke: the refresh token is kept in the Keychain at sign-out and retried at the next launch. Dropped after 5 failed launches or 30 days. Code: `apps/macos/FsyncMac/.../Bridge/PendingRevoke*.swift`.
- Windows pending revoke: same rules, stored in the DPAPI secret store (`PendingRevoke.cs`, `SecretStore.PendingRevoke.cs`, `EngineBridge.Revoke.cs`).
- Relay lists accept comma-separated URLs: macOS and Windows `FSYNC_RELAY` (macOS also Info.plist `FsyncRelayUrl`); `apps/macos/scripts/build-fsync-mac-app.sh --relay URL[,URL]`.
- Windows C# bindings regenerated for `relay_urls`, `logout_confirmed` and `revoke_session`.

### Changed
- OTP send failures now follow the outcome: a definite failure (refused, TLS/auth, rejected) deletes the code and refunds the slot and quotas. A timeout keeps the code and counts. See [api](./api.md#auth).
- Ohio full stack moved to release `519ec821e90f` with migrations 0001-0003.
- Windows sign-out always completes locally, even when the pending revoke save or the logout call fails. The pending revoke is saved under one lock (fix `27ed996`).
- macOS sign-out settles only the pending revoke that this sign-out saved (a newer one is not cleared by an older retry).

### Verification
- Ohio: real OTP through Gmail SMTP, `otp/start` 204 in 1.8 s (2026-10-15).
- Singapore relay e2e (client in Vietnam): relay-only clip 0.42 s; 8 MiB via relay at 0.63 MiB/s, hash OK. Ohio relay measured earlier at about 0.105 MiB/s, so about 6x.
- E2E with both relays configured (`relay-fsync.10z.one`, `relay-sg-fsync.10z.one`): relay-only 8 MiB at 0.55 MiB/s; relay-only clip 1.18 s.
- Tests: `dotnet` 144/144; `cargo` 300 passed, 0 failed; `swift` 208.

### Open
- Phase 14 manual GUI QA (Windows app) not done.
- Alert on the log line `global OTP send cap reached` (alerting setup is not in the repo).
- Hard logout after quit is closed for the macOS and Windows apps only when the app launches again. If it is never launched again, the refresh token stays valid until its 30-day expiry. The core retry queue stays in memory (unchanged). The backend needed no change: the revoke was already persisted (`refresh_tokens.revoked_at`).

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
