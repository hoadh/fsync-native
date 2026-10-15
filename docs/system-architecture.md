# System Architecture

Last updated: 2026-10-15. Covers the P0 Rust track (phases 01, 02, 06-09) with its hardening (read-ahead, logout revocation, data-dir lock, multiple relays), the production deployment (phase 10), the macOS FFI package, the macOS app (phases 04, 11, 12) and the Windows app (phases 03 C# half, 13, 14). Windows phase 05 is partial and phases 15-16 are not started; see [development-roadmap](./development-roadmap.md).

## Overview

fsync syncs clipboard items and files between a user's own devices. Trust is rooted in device keys, not in the account:

- **Backend** (`backend/`): email-OTP auth, a signed device-list registry, pairing-code storage, and presence (which devices are online and where they can be reached). It never sees clipboard or file bytes.
- **Core** (`core/`): the Rust engine. It verifies the device-list chain, pairs devices, moves clips and files over iroh QUIC, and keeps encrypted local history.
- **Apps** (`apps/`): thin UI layers over the core through UniFFI. macOS (`apps/macos/Packages/FsyncCoreKit`, the `TrustResetPrompt` library, and the `apps/macos/FsyncMac` menubar app with clipboard and file sync) is built; Windows is partially built (see the roadmap).

```
  Backend (axum + Postgres 17)           metadata only: accounts, device lists, pairing codes, presence
    ^ HTTPS/JSON /v1/*          ^ WSS /v1/presence (challenge, auth, hello, peer_update)
    |                           |
  Device A                    |                       Device B
  App -> FsyncCoreKit (Swift) |                       fsync-cli (or app via FFI)
      -> fsync-ffi -> fsync-core service ...  <== iroh QUIC ==>  fsync-core service
  ALPNs: fsync/pair/1 (pairing), fsync/clip/1 (clip offers), fsync/file/1 (file fetch)
  Path: direct (LAN / hole punch) or iroh relay (sees ciphertext only)
```

## macOS app shell

`apps/macos/FsyncMac` is a SwiftPM menubar app (phase 11) over `FsyncCoreKit`. Targets: `FsyncMacCore` (logic) -> `FsyncMacUI` (SwiftUI) -> `FsyncMac` (`@main`).

- **EngineBridge** (`@MainActor`) is the only door to the engine. Blocking FFI calls run off the main actor (`EngineCall`); engine callbacks hop to the main queue in FIFO order. `AppPhase` (booting, signedOut, needsPairing, ready, bootFailed) is derived from `AccountState`.
- **Secrets** are in the Keychain only (device secret, history key, session). The data-protection Keychain needs `application-identifier` + `keychain-access-groups`, which only a Developer ID build with its provisioning profile carries; an ad-hoc build falls back to the file keychain (probed once). Writes tolerate a duplicate-item race between the login model and the engine's session callback. Details:
  - **Probe**: `useDataProtection` adds a probe item once per process; `errSecDuplicateItem` (a probe left by a crashed run) counts as supported.
  - **Upgrade path** (verified with a Developer ID build): if a later signed build can use the data-protection Keychain, a `get` miss there reads the file keychain and moves the item over (`readMigrating`), so `device_secret`, `history_key` and the session survive the switch. A failed copy still returns the secret and keeps the old item for the next read. `delete` removes both copies, so a signed-out session cannot come back from the old store.
  - **Serialized**: every `KeychainStore` call takes one process-wide recursive lock. The file keychain can deadlock in `SecItemAdd` when two threads add the same item at once (hit by a concurrent-first-write test).
  - **Keys are add-only**: `loadOrCreateKey` never overwrites; on a duplicate it reads back the winner.
  - **Stale session callbacks**: `EngineDelegateAdapter` stores refreshed tokens only while a `SessionGate` is open (stored session at boot, or after sign-in); sign-out and `requireSignIn` close it, so a refresh queued in the core cannot restore a signed-out session.
- **Flows**: email + OTP login; first device (`Created`); pairing in both roles with a SAS check bound to the displayed request id; device list with remove (confirmed); recovery via `reset_account_trust` behind a two-step confirm; trust-reset prompt wired from `AccountEvent.trustReset`; launch at login (`SMAppService.mainApp`, bundle only); pairing codes are copied with the `ConcealedType`/`TransientType` pasteboard markers.
- **Status**: the core reports Lan/Relay only for a live QUIC connection. Until something syncs, a peer the backend sees online shows as "Online" (`Device.online`), not Lan.
- **Packaging**: `build-fsync-mac-app.sh` assembles an `LSUIElement` bundle, with the hardened runtime: ad hoc with no entitlements by default, or Developer ID with an embedded profile and the Keychain entitlements (`--sign`/`--profile`), optionally notarized (`--notary-profile`).

## macOS clipboard and files

Mac-side logic lives in `FsyncMacCore/Clipboard`, `Files`, `Settings` and `Features/History`; the engine does the transport. Decision record: [ADR 009](./adr/009-macos-lazy-paste.md).

### Clipboard send (auto mode)

1. `PasteboardWatcher` polls `changeCount` and reports changes to `ClipboardSync`.
2. `SensitiveFilter` decides before the content is read. It skips copies marked as our own write (`com.fsync.origin`), concealed (`org.nspasteboard.ConcealedType`, 1Password's type), transient, auto-generated, and copies while a denylisted app is frontmost. The built-in denylist holds six password managers; the user's list is added on top.
3. Every step is guarded by `changeCount`: if the pasteboard changed before the snapshot, during the read, or before the send, the clip is dropped (no stale send). Explicit sends also re-check the sensitive types.
4. `RepresentationMapper` builds text/plain, text/html, text/rtf and image/png representations. TIFF is converted to PNG off the main thread. A clip over `maxClipBytes` is refused (default 50 MiB, minimum 1 MiB); an oversized TIFF is refused without decoding.
5. The clip goes to the engine as an offer (see [data flow 4](#4-clip-fsyncclip1)). The engine records it in history and sends it to paired devices.
- `⌃⌥C` sends the current clipboard explicitly. Explicit sends skip the denylist (ADR 009).

### Clipboard receive

- `ClipboardWriter` puts received representations on the pasteboard. Representations up to 1 MiB are fetched before the write; larger ones are served when read.
- Writes are tagged `com.fsync.origin`, so the watcher ignores them (echo guard).
- In manual mode, the newest remote clip waits as "Paste from <device>" until the user asks for it.

### File clips

- Received files of a clip follow `FileClipStrategy`. Default `hybrid`: files under 20 MiB are fetched before the pasteboard write, larger files are served when a reader asks for their file URL (`LazyFileProvider`). Eager bytes per clip are capped at 512 MiB; files over the cap are lazy. `lazy` and `eager` are settings too.
- Eager files land in `~/Library/Caches/fsync/inbox/<item id>/` (folder mode 0700). Item ids from other devices are checked as plain hex and dashes before they become folder names.
- A lazy entry cannot be served after the app quits, so `FileClipWriter` removes it on quit. A blocking fetch on the main thread gives up after 180 s.
- Retention (`InboxDirectory.clean`) runs at start, when the retention settings change, and daily. It removes inbox folders older than `retentionDays` (default 30), then the oldest until the folder is under `retentionBytes` (default 1 GiB). The folder of the current clip is kept.

### Incoming offers and transfers

- "Send to…" and drop on a device row call `FilesSync.send`. The receiver must accept.
- An incoming offer waits for the user unless auto-accept is on and the offer is 1 GiB or less. The list keeps the 20 newest offers.
- Accept downloads each file into the Downloads folder. `TransfersModel` shows progress (last 50 rows) and cancel. Cancel stops the rest of the offer and does not count as a failure.

### History and settings

- `HistoryModel` pages 50 items, with search, pin, delete and clear (pinned items kept by default). History keeps 200-character previews; copy-back works only for text stored in full.
- `ClipSettingsModel` loads settings off the main actor. Edits are refused until the load succeeds. Defaults: auto mode, 30-day retention, 1 GiB size cap, 50 MiB max clip, auto-accept off. Settings are stored by the core.

## Repository crates

| Crate / unit | Path | Role |
|---|---|---|
| `fsync-proto` | `core/fsync-proto` | Wire types only, no I/O: `DeviceList` and `SignedDeviceList` (postcard, domain-separated ed25519), chain verification, frames (`MAX_FRAME_LEN` = 1.25 MiB), clip/file/pair messages, presence DTOs, name and shape checks. |
| `fsync-core` | `core/fsync-core` | Engine. Modules below. |
| `fsync-ffi` | `core/fsync-ffi` | UniFFI 0.29.4 adapters over `FsyncService`. No business logic. Contract v2 (`contract_version()` returns 2) in [ffi-contract](./ffi-contract.md). |
| `fsync-cli` | `core/fsync-cli` | Test and demo harness. JSON lines on stdout. Drives the same service facade as the apps. |
| `fsync-backend` | `backend` | Lib + bin. Auth, device registry, pairing, presence. |
| iroh spike | `spikes/iroh-discovery` | Standalone (not a workspace member). Provides the `relay` subcommand the e2e demo uses. |

### fsync-core modules

| Module | Responsibility |
|---|---|
| `service/` | `FsyncService`: the facade apps call. Login, account, pairing (both sides), devices, clip publish, file send/fetch, history, settings, events to one delegate thread. |
| `engine/` | Orchestration below the facade: outbox, placement of inline vs spilled reps, shared state, history hooks. |
| `api/` | Backend HTTP and WebSocket client: token refresh, device-list sync, pairing create and lookup, presence session (`Hello` on relay readiness and address change, 20 s heartbeat, backoff). |
| `net/` | iroh node builder and `Node`: accept gate (only pinned devices), discovery fed from presence (trusted peers only), path tracking (`Lan` / `Relay`), dialer. |
| `pair/` | `fsync/pair/1`: SPAKE2 (code mode) or HKDF (QR mode), MAC confirm, 3-word SAS, pairing window with admission limits. |
| `trust/` | Pinned device-list head and root, chain ingest, reset detection and confirmation, trust stores (`SqliteTrustStore` in history, `FileTrustStore` for CLI/tests). |
| `clip/` | `fsync/clip/1`: send and receive, echo guard, offer validation, per-peer rate limit, received-offer log. |
| `file/` | `fsync/file/1`: serve and fetch, CV (chunk-group) outboard via `blake3::hazmat`, resume from `.part`, ranged reads, size and deadline policy. |
| `blobs/` | `BlobStore` trait, `FsBlobStore`, `EncryptedBlobStore` (FSB1 format). |
| `history/` | SQLCipher database: items, reps, files, settings, trust. Retention, recovery, migrations. |
| `identity`, `config`, `settings`, `events`, `sync` | Device key, engine config, user settings, event types, device-list sync and presence pump. |

## Backend

- Stack: axum 0.8, sqlx 0.8 on Postgres 17, all routes nested under `/v1` (`backend/src/lib.rs`). `GET /health` is outside the `/v1` nest and runs `SELECT 1`.
- Routes:

| Area | Routes |
|---|---|
| Auth | `POST /v1/auth/otp/start`, `POST /v1/auth/otp/verify`, `POST /v1/auth/refresh`, `POST /v1/auth/logout`, `DELETE /v1/account` |
| Device list | `GET /v1/device-list`, `PUT /v1/device-list` (optional trust-reset header), `GET /v1/device-list/chain` |
| Pairing | `POST /v1/pairing`, `GET /v1/pairing/{code}` |
| Presence | `GET /v1/presence` (WebSocket) |

- Tables (`backend/migrations/0001_init`, `0002_refresh_session`, `0003_otp_send_log`): `accounts`, `otp_codes`, `refresh_tokens`, `device_lists`, `device_list_history`, `pairing_codes`, `otp_send_log`.
- Behaviour that matters to clients:
  - OTP codes, pairing codes: stored as keyed hashes. Pairing codes are bound to the account.
  - Access token: EdDSA JWT, 15 min. Each request checks that its refresh family is still live.
  - Trust reset and account delete need auth from the last 5 min (`reauth_required`).
  - Device-list history keeps the last 100 rows per account and epoch. Clients fetch from genesis or pin a head.
  - Presence: in-memory hub, one bounded queue per socket (drop on full). Membership is rechecked before a socket registers and before `peers` is sent.
  - Rate limits and the unauthenticated-socket cap are in memory, so they reset on restart and do not span instances.
- Deployment: `deploy/backend/Dockerfile` (build context = repo root). The production stack is `deploy/compose.yml` (see [deployment topology](#deployment-topology)); `deploy/backend/compose.yml` is the older dev-only file. Config comes from env; see [api](./api.md#config-env).

## Relay

- The core never uses n0 default relays. Relays come from config (`NodeBuilder::relays`). An empty list disables relays.
- Production relays: `deploy/relay/` builds an image from `iroh-relay` 1.3.0 (pinned to `Cargo.lock`). Two regions, each published on loopback `18081` behind its host's Caddy (no buffering: `flush_interval -1`):
  - US-East-2 (Ohio): `relay-fsync.10z.one`, in the full compose stack with the backend.
  - ap-southeast-1 (Singapore): `relay-sg-fsync.10z.one`, relay-only host (`deploy.sh --relay-only`), no backend or Postgres.
- **Clients use both relays.** The macOS and Windows apps pass both URLs (`relay_urls`, comma-separated in `FSYNC_RELAY` or `--relay`). The core merges them into one iroh `RelayMap` (`net/relays.rs`); the endpoint probes all of them and picks the home relay by latency, and moves it when the network changes. A device reports its current home relay in presence.
- **Relay failover, what is verified.** Connect-time failover: with one of two relays unreachable, peers still connect (`core/fsync-core/tests/relay_failover.rs`). Mid-run relay death (the home relay dies while connected): **not verified**. The in-process test relays lack QUIC address discovery, so iroh never re-homed within 45 s. **Relays do not mesh:** two peers whose only reachable relays differ cannot connect to each other.
- The backend runs only in Ohio.
- QUIC address discovery (QAD, udp/7842) is off by default. With `RELAY_QAD=1` the relay needs the Caddy-managed certificate for its own hostname, mounted alone.
- For local runs and the e2e demo, `spikes/iroh-discovery` starts an in-process `iroh-relay` in HTTP dev mode (`relay [port]`).
- Relay sees only QUIC ciphertext. Clip and file bytes never reach the backend.

## Deployment topology

One VPS shared with other services. Compose project `fsync`, files in `deploy/`. Runbook: [deploy/README.md](../deploy/README.md). Short guide: [deployment guide](./deployment-guide.md).

```
internet --443--> host Caddy (TLS, ACME; not part of the compose file)
                    api-fsync.10z.one   -> 127.0.0.1:18080  backend
                    relay-fsync.10z.one -> 127.0.0.1:18081  relay
internet --udp/7842--> relay QAD (optional, off by default)

compose network (pinned subnet, FSYNC_SUBNET)
  backend  --> postgres:17 (no host port)
  relay    (metrics stay inside the network)

Singapore relay-only host (no backend, no Postgres)
internet --443--> host Caddy (owner-managed)
                    relay-sg-fsync.10z.one -> 127.0.0.1:18081  relay
```

- **Client IP trust.** Caddy reaches the backend through the published loopback port, so the backend sees the subnet gateway (or 127.0.0.1). `TRUSTED_PROXIES` names that subnet plus loopback and `TRUST_PROXY_HOPS=1`, so `X-Forwarded-For` is honored only from those peers. `smoke-check.sh` with `CHECK_PROXY_TRUST=1` verifies this on the host.
- **DNS.** Both hostnames must be DNS-only (not proxied). The relay needs long-lived upgraded connections and UDP. Behind a proxy, per-IP limits would bucket by the proxy address.
- **Ports.** Nothing is published beyond loopback except the optional QAD port.
- **Images.** Built on the operator machine for `linux/amd64` (cross-compiled; no Rust compile under emulation), then shipped with `docker save | docker load` only for tags the host lacks. Releases are git-sha tags; the host keeps the current and the previous release.
- **Deploy.** `deploy.sh` takes a remote `flock`, stages into a unique directory, rewrites `.env` atomically, keeps `compose.yml.prev` for `--rollback`, and waits for container health plus a loopback smoke check. Rollback restores images and compose file, not data. Migrations are forward-only, so `--rollback` first runs `scripts/rollback-guard.sh`: it refuses a target release whose migrations do not include the database's current max version (`--force-rollback` overrides it, after reading the script). A rehearsal without this guard crash-looped the backend on a release that predates migration 0003.
- **Health probe.** `.github/workflows/health-probe.yml` runs `deploy/scripts/probe-health.sh` hourly. It checks `api /health`, `/health/otp` (404 is a warning on old releases), both relays' `/healthz`, and certificate expiry (>= 14 days). A failed run is the alert. It runs only while GitHub Actions is enabled.
- **Off-box backups.** Nightly at 03:15 on the app host: `pg_dump`, gpg encryption with a public key on the host, then sftp upload to the Singapore backup host, whose upload-only user cannot delete stored files. Ingest runs there every 10 min; retention 30 days, minimum 3 files. The private key is held off both hosts. Failure is visible only in the host's `backup.log` unless `BACKUP_PING_URL` is set. Restore: [deploy/operations.md](../deploy/operations.md).
- **Limits.** Memory limits are sized for a shared host of about 2 GiB: Postgres 192 MiB, backend 96 MiB, relay 96 MiB, no swap.
- **Local backups.** `backup-postgres.sh` writes `pg_dump -Fc` dumps (14 days, refuses below 1 GiB free); the nightly cron runs it through `backup-offsite.sh` (see Health probe and Off-box backups above). `restore-postgres.sh` restores one dump.
- **CI.** `ci.yml` runs the Rust job always and the macOS and Windows jobs only when their paths change (always on push to `main` and on dispatch). `deploy-stack.yml` builds both images and runs `tests/e2e/compose-stack-sync.sh` when deploy inputs change. Actions are pinned by SHA. **GitHub Actions is currently blocked by billing:** the latest macOS-round runs never started, so no workflow (CI, deploy-stack, health probe) runs until it is unblocked.
- **Status (2026-10-15).** Ohio runs the full stack behind host Caddy (public HTTPS, migrations 0001-0003). Its release is `f00980db3978` (main), with `549cc6a3d298` as the rollback target. OTP email goes out through Gmail SMTP, verified 2026-10-15. Singapore runs the relay only (release `519ec821e90f`, `.mode` = `relay-only`).
- **Relay-only mode.** `deploy.sh --relay-only` ships only the relay image and `compose.relay-only.yml`. It writes `.mode` on every deploy, so a full deploy cannot land on that host and a relay-only deploy cannot replace a full stack.

## Data flows

### 1. Account bootstrap

1. `otp/start` then `otp/verify` (email code). First login creates the account.
2. First device: creates a signed genesis `DeviceList` (seq 0) and `PUT`s it with `If-Match: none`. Login reports `account: created`.
3. Later devices: login reports `account: needs_pairing`. They must pair before they can sync.

### 2. Pairing with SAS

The new device (N) opens a pairing window and shows a code or QR. The existing device (E) joins.

1. N creates the offer. Code mode: `POST /v1/pairing` stores the hashed code and the node id. QR mode: the secret stays on the devices.
2. E looks up the code (`GET /v1/pairing/{code}`, same account only, single use) to get N's node id and relay URL. In QR mode E has the node id from the payload.
3. E dials N on `fsync/pair/1`. Keys come from SPAKE2 (code) or HKDF (QR) over a transcript that includes both 16-byte nonces. Both sides check a MAC.
4. Both devices show the same 3-word SAS. Each user must accept. N pins nothing until its user accepts. E publishes only after its user accepts and N sends `Accept`.
5. E publishes the new list (`seq + 1`, `If-Match` = stored seq). N pins the head it receives. Backend pushes `device_list_changed` to the account's sockets.
6. If the node id is already listed (re-pair after a history reset), E skips the publish and reuses the pinned head.

Limits: at most 3 unauthenticated sessions per window, one attempt per source node id, 3 MAC failures close the window, and the window is claimed by the first session that passes the MAC.

### 3. Presence and address discovery

1. Server sends `challenge`. Client replies `auth` with its access token and an ed25519 signature over `"fsync-presence-v1" || nonce`.
2. Server replies `peers` (the device's view). A node that is neither listed nor holding a live offer is closed with 4401.
3. Client sends `hello` with its relay URL and direct addresses. It waits up to 300 ms for the relay URL before the first `hello`, re-sends on address change (200 ms debounce), and sends a heartbeat every 20 s.
4. Backend fans out `peer_update` to listed devices. Core feeds the addresses into iroh discovery, for trusted peers only.

### 4. Clip (`fsync/clip/1`)

1. `publish_clip` builds a `ClipOffer`: each rep is hashed (blake3). Reps are inlined while the 256 KiB budget lasts. Reps over 1 MiB spill to the blob store. Files are snapshotted (size, mtime, blake3).
2. The item is recorded in history (unless sensitive) and placed in the outbox before any send.
3. One uni stream per target peer, each with one framed offer. Sends have a 5 s per-peer timeout. Delivery counts when the peer acks the frame.
4. Receiver: checks that the offer origin matches the connection peer, validates it (versions, hashes, budgets, `rel_path` safety), deduplicates by `item_id`, records history, then emits `RemoteClip`.
5. Latency in the last e2e runs: about 0.02-0.03 s on LAN and on relay. The bound in the demo is 5 s.

### 5. File transfer (`fsync/file/1`)

1. The sender offers files inside the clip offer. The receiver calls `fetch_file_to`, which runs a bi-stream `FetchReq` (aligned offset, `no_cvs` flag) and gets `FetchResp` (`Ok`, `NotFound`, `Changed`, or `Denied`).
2. Chunk-group CVs (16 KiB groups, blake3 hazmat) let each group be verified as it arrives. The merged tree must equal the offered hash.
3. Bytes land in `<hash>.part`, which is resumable at 16 KiB boundaries. On completion the data is finished, then exported to the chosen folder with no clobber.
4. Policy checks run before any transfer: declared size against free space and `max_file_bytes`. CV buffer cap is 64 MiB. Deadline is 120 s plus size / 256 KiB/s, with a 30 s stall timeout.
5. **Read-ahead** (`read_file_range`, `file/range_reader.rs`). A reader that takes small chunks one after another (the Windows shell reads 256 KiB at a time, one in flight) would otherwise pay one round trip per 4 MiB window. After two sequential reads in a row, up to 4 windows are fetched ahead, and at most 8 windows are held per reader. A first read is never sequential; a seek resets the run; windows behind the reader are dropped. Speculative fetches take a per-peer permit (8 per peer, shared by the peer's readers) and never wait, so a foreground read never queues behind them. Measured at 50 ms RTT: 46 → 105 MiB/s. Random reads are unchanged.

### 6. Trust reset

1. `reset_account_trust` signs a new seq-0 genesis (this device only) and `PUT`s it with the reset header (fresh auth required, 3 per day; stale login -> `ReauthRequired`). It then replaces the local pin.
2. Old devices see a different root on their next sync and get `EngineEvent::TrustReset` (service: `AccountEvent::TrustReset { reset_id, devices, device_names }`, `reset_id` = hash of the offered head, so a grown offer is announced again and an old id is `NotFound`). They never auto-accept it: `confirm_trust_reset` pins it (a device that is not in the new list gets `SelfRemoved`, drops its pin and must re-pair), `decline_trust_reset` keeps the old pin. See ADR 012.

### 7. Logout

1. The core takes the refresh token, then signs out locally: session, tokens, pinned device list and presence. `logout()` returns `Ok` after this step, even when the network is down.
2. One inline revoke attempt (`POST /v1/auth/logout`), capped at 3 s. The backend revokes the whole refresh family, so rotated tokens die with it.
3. If there is no answer, or the answer is 408, 429 or 5xx, the revoke is retried in the background: 5 s, doubling to 5 min, while the service object lives. Other answers end the retries (401 means already revoked).
4. Tokens are never persisted by core. `logout_confirmed()` reports whether the revoke settled. If the app quits first, the host keeps a pending revoke (macOS Keychain, Windows DPAPI) and calls the engine-free `revoke_session` at the next launch; it is dropped after 5 failed launches or 30 days. See [FFI contract](./ffi-contract.md#hard-logout-after-quit-host-pattern).

### 8. Redial after a peer restarts

A peer that crashes and comes back with the same node id on a new port used to stay unreachable for 15-30 s: iroh kept a stale selected path while old connections were still alive, so new handshakes went to the dead path until idle timeout.

1. The first dial attempt has a 2 s budget (`net/dialer.rs`, `FIRST_ATTEMPT`).
2. On stall, outbound same-ALPN connections opened by the shared `Dialer` are closed as stale (`Gate::close_stale`, code 410). Other ALPNs are left alone.
3. The dial retries with the current addresses (`Discovery::addr_for`). Measured: about 2 s. Covered by `core/fsync-core/tests/dial_after_restart.rs` (crash, graceful restart, ALPN scoping).
4. Gap: the file and rep fetch paths do not retry on a dead cached connection. Clip send does.

## Storage

### Backend (Postgres)

| Table | Holds |
|---|---|
| `accounts` | Account id, email (lowercased) |
| `otp_codes` | Keyed hash of the OTP, attempts, expiry |
| `otp_send_log` | One row per OTP email sent, for the global 24 h cap (`OTP_DAILY_SEND_CAP`); a definite send failure deletes its row |
| `refresh_tokens` | Hashed refresh tokens by family (`auth_time`, `rotated_at`) |
| `device_lists` | Current signed list per account and epoch |
| `device_list_history` | Signed lists for chain fetch (last 100 per epoch) |
| `pairing_codes` | Keyed hash of the code, bound to account, expiry |

### Client data directory

| File (CLI layout) | Content |
|---|---|
| `identity.key` | Device signing seed, 0600. Apps pass the 32 bytes as `device_secret` (Keychain or DPAPI). |
| `history.key` | 32-byte history key, 0600. Apps pass `history_key` from Keychain or DPAPI. |
| `session.json` | Access and refresh tokens, 0600. |
| `engine.lock` | Exclusive lock (`File::try_lock`) held while an engine runs. A second engine on the same directory fails to start with `Invalid`. Released on `stop()` or drop; a crash never leaves it held. |
| `core/history.db` | SQLCipher database (raw 32-byte key, WAL, `secure_delete`). Tables: `items`, `reps`, `files`, `settings`, `trust`, plus the search index. |
| `core/history.keycheck` | Keyed blake3 tag of the history key. Tells a wrong key apart from a damaged file. |
| `core/blobs/` | `<hash>` (encrypted) and `<hash>.part` (staging). |

Blob format (`FSB1`): 19-byte random nonce prefix, then 64 KiB STREAM-BE32 segments. XChaCha20-Poly1305, AAD is the plaintext blake3. Subkeys come from HKDF-SHA256 with salt `fsync-blobs-v1`. Random-access reads work and truncation fails authentication.

### Key loss and recovery

| Situation | Result |
|---|---|
| Key differs from the one that created the DB | `HistoryKeyMismatch`. Nothing is moved. The right key still opens it. |
| DB damaged (same or unknown key verdict) | Files moved to `history.corrupt-<ts>*`, fresh DB, `HistoryReset` event. The newest 3 backups are kept, and backups that hold data are never pruned. |
| Explicit reset | `reset_history(data_dir)` (FFI) or `fsync-cli history reset --yes`. Moves the DB aside. Pinned trust in the history is lost, so the device must re-pair. |

### Retention

Defaults (settings): history mode Auto, 30 days, 1 GiB size cap, 50 MiB max clip. Order: age, then size cap (oldest unpinned first, skipping items that free nothing), then orphan blob GC with a 1 h grace. Stale `.part` files are swept after 24 h. Pinned items are never deleted. Retention runs at start and hourly.

## Threat model highlights

| Threat | Mitigation | Residual risk |
|---|---|---|
| Backend reads or alters clips and files | Clip and file bytes go only over iroh QUIC, and never reach the backend. | Metadata (accounts, device list, presence) is visible to the backend. |
| Backend adds a rogue device | Clients verify the signed chain from a pinned root. The server checks defensively but is not trusted. | A malicious backend can still show a fake code in code mode. The SAS catches this only if the users compare words. QR mode avoids it. |
| Network MITM during pairing | SPAKE2 or HKDF with nonces, MAC confirm, 36-bit SAS on both devices, explicit consent. | 36-bit SAS. A first connector can burn a window (DoS only). |
| Untrusted peer connects | Accept gate: only pinned node ids. Live connections for a removed device close with 403. | None known. |
| Stolen auth tokens | 15 min access tokens, rotating refresh with reuse detection, family revoke on logout or reuse. Logout revokes server-side, retrying in the background. | A stolen live access token works until it expires (about 15 min). A refresh token survives an app quit that comes before its revoke reaches the backend. The macOS and Windows apps keep a pending revoke and retry it at launch; if the app is never launched again, the token lives until its 30-day expiry. |
| Account takeover then device-list change | Trust reset and account delete need fresh auth (5 min). Resets are capped at 3 per day. Old devices get `TrustReset`. | An email takeover can still trigger a reset; the user of each old device must confirm or decline it (`confirm_trust_reset` / `decline_trust_reset`). |
| Disk theft or same-user read of history | SQLCipher with the Keychain/DPAPI key. Blobs encrypted. | Received files exported to user folders are plaintext by design. `.part` staging is plaintext until finish. |
| Sensitive clip stored | Sender does not store it. `ClipOffer.sensitive` tells the receiver to record nothing; blobs it fetched for that item are removed after use. | Honouring the flag is the receiver's behaviour. A modified receiver can ignore it. |
| Sensitive pasteboard content sent from the Mac | `SensitiveFilter` skips concealed, transient, auto-generated and denylisted-app copies before any read. Our own writes are not re-sent. | Best effort: apps that do not mark secrets are sent. The denylist does not apply to explicit `⌃⌥C` sends. |
| Oversized or malicious input | Frame length checked before allocation, `ClipOffer::validate`, file size and CV caps, per-peer token bucket (20/s), receive log budget (32 MiB), shape and name checks. | Clip size limit is user-settable (`max_clip_bytes`, applied live). Other limits are fixed defaults. |
| Abuse of backend endpoints | Per-IP and per-email OTP limits, per-account pairing and PUT limits, header read timeout, in-flight cap, socket caps. | Limits are in memory and per instance. |

## Known gaps

- Presence and rate-limit state is per instance. The backend is single-instance.
- Relay: mid-run failover (home relay dies while connected) is unverified; relays do not mesh, so peers on disjoint relays cannot connect ([Relay](#relay)).
- Redial: file and rep fetches do not retry on a dead cached connection (clip send does) ([Redial](#8-redial-after-a-peer-restarts)).
- No Postgres TLS (`tls-rustls` not enabled in sqlx).
- Email: Gmail SMTP is the only real sender. Gmail's consumer limit (about 500 recipients per day) is enforced by `OTP_DAILY_SEND_CAP` (450 per rolling 24 h). A transactional provider is needed before real traffic (only the `SMTP_*` values change).
- Root of a fresh device's chain is learned only when a chain through the head is seen. An old genesis replayed to a fresh device triggers a reset prompt, not a silent accept.
- The denylist setting is stored and validated, but the core does not enforce it. Apps must filter before publishing.
- Windows (phases 03 C#, 05, 13, 14) is partial: phase 14 is merged, manual GUI QA is open, and ARM64 is not run. Phase 10 has two relay regions; the backend is still single-region (Ohio).
- Logout after an app quit: `revoke_session` and the pending revoke (macOS, Windows) are built. Still open: a host that is never launched again keeps the refresh token until its 30-day expiry. The core retry queue itself is in memory.
- Public relay throughput from Vietnam to the US-East-2 VPS is about 0.1 MiB/s, the same as the raw path. A Vietnam client measured about 0.63 MiB/s (8 MiB, relay-only) through the Singapore relay, about 6x the Ohio figure.

## Related docs

- [Deployment guide](./deployment-guide.md): what runs where and the deploy rules
- [Protocol spec](./protocol-spec.md): wire frames, ALPNs, limits, measured numbers
- [API](./api.md): backend HTTP and WebSocket contract
- [FFI contract](./ffi-contract.md): UniFFI surface (contract v2) and error mapping
- [Code standards](./code-standards.md): conventions and quality gates
- [ADRs](./adr/): decisions 001-012
- [Codebase summary](./codebase-summary.md): map of the repository
