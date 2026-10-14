# System Architecture

Last updated: 2026-10-14. Covers the P0 Rust track (phases 01, 02, 06, 07, 08, 09), the macOS FFI package, and the macOS app shell (phase 11). Phases 03 (Windows half), 04, 05, 10 (relay deploy), and 12-16 are not built yet; see [development-roadmap](./development-roadmap.md).

## Overview

fsync syncs clipboard items and files between a user's own devices. Trust is rooted in device keys, not in the account:

- **Backend** (`backend/`): email-OTP auth, a signed device-list registry, pairing-code storage, and presence (which devices are online and where they can be reached). It never sees clipboard or file bytes.
- **Core** (`core/`): the Rust engine. It verifies the device-list chain, pairs devices, moves clips and files over iroh QUIC, and keeps encrypted local history.
- **Apps** (`apps/`): thin UI layers over the core through UniFFI. macOS (`apps/macos/Packages/FsyncCoreKit`, the `TrustResetPrompt` library, and the `apps/macos/FsyncMac` menubar app shell) is built; Windows is not started.

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

- Tables (`backend/migrations/0001_init`, `0002_refresh_session`): `accounts`, `otp_codes`, `refresh_tokens`, `device_lists`, `device_list_history`, `pairing_codes`.
- Behaviour that matters to clients:
  - OTP codes, pairing codes: stored as keyed hashes. Pairing codes are bound to the account.
  - Access token: EdDSA JWT, 15 min. Each request checks that its refresh family is still live.
  - Trust reset and account delete need auth from the last 5 min (`reauth_required`).
  - Device-list history keeps the last 100 rows per account and epoch. Clients fetch from genesis or pin a head.
  - Presence: in-memory hub, one bounded queue per socket (drop on full). Membership is rechecked before a socket registers and before `peers` is sent.
  - Rate limits and the unauthenticated-socket cap are in memory, so they reset on restart and do not span instances.
- Deployment: `deploy/backend/` has a Dockerfile (build context = repo root) and `compose.yml` (Postgres 17 + backend). Config comes from env; see [api](./api.md#config-env).

## Relay

- The core never uses n0 default relays. Relays come from config (`NodeBuilder::relays`). An empty list disables relays.
- Production relay is not deployed. `deploy/relay/` is empty; phase 10 plans two regions (SG and US/EU).
- For local runs and the e2e demo, `spikes/iroh-discovery` starts an in-process `iroh-relay` in HTTP dev mode (`relay [port]`).
- Relay sees only QUIC ciphertext. Clip and file bytes never reach the backend.

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

### 6. Trust reset

1. `reset_account_trust` signs a new seq-0 genesis (this device only) and `PUT`s it with the reset header (fresh auth required, 3 per day; stale login -> `ReauthRequired`). It then replaces the local pin.
2. Old devices see a different root on their next sync and get `EngineEvent::TrustReset` (service: `AccountEvent::TrustReset { reset_id, devices, device_names }`, `reset_id` = hash of the offered head, so a grown offer is announced again and an old id is `NotFound`). They never auto-accept it: `confirm_trust_reset` pins it (a device that is not in the new list gets `SelfRemoved`, drops its pin and must re-pair), `decline_trust_reset` keeps the old pin. See ADR 012.

## Storage

### Backend (Postgres)

| Table | Holds |
|---|---|
| `accounts` | Account id, email (lowercased) |
| `otp_codes` | Keyed hash of the OTP, attempts, expiry |
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
| Stolen auth tokens | 15 min access tokens, rotating refresh with reuse detection, family revoke on logout or reuse. | A stolen live access token works until it expires (about 15 min). |
| Account takeover then device-list change | Trust reset and account delete need fresh auth (5 min). Resets are capped at 3 per day. Old devices get `TrustReset`. | An email takeover can still trigger a reset; the user of each old device must confirm or decline it (`confirm_trust_reset` / `decline_trust_reset`). |
| Disk theft or same-user read of history | SQLCipher with the Keychain/DPAPI key. Blobs encrypted. | Received files exported to user folders are plaintext by design. `.part` staging is plaintext until finish. |
| Sensitive clip stored | Sender does not store it. `ClipOffer.sensitive` tells the receiver to record nothing; blobs it fetched for that item are removed after use. | Honouring the flag is the receiver's behaviour. A modified receiver can ignore it. |
| Oversized or malicious input | Frame length checked before allocation, `ClipOffer::validate`, file size and CV caps, per-peer token bucket (20/s), receive log budget (32 MiB), shape and name checks. | Clip size limit is user-settable (`max_clip_bytes`, applied live). Other limits are fixed defaults. |
| Abuse of backend endpoints | Per-IP and per-email OTP limits, per-account pairing and PUT limits, header read timeout, in-flight cap, socket caps. | Limits are in memory and per instance. |

## Known gaps

- Presence and rate-limit state is per instance. The backend is single-instance.
- No Postgres TLS (`tls-rustls` not enabled in sqlx).
- Email delivery is a log sender only.
- Root of a fresh device's chain is learned only when a chain through the head is seen. An old genesis replayed to a fresh device triggers a reset prompt, not a silent accept.
- The denylist setting is stored and validated, but the core does not enforce it. Apps must filter before publishing.
- Windows (phases 03 C#, 04, 05, 13, 14) and the relay deploy (phase 10) are not done.

## Related docs

- [Protocol spec](./protocol-spec.md): wire frames, ALPNs, limits, measured numbers
- [API](./api.md): backend HTTP and WebSocket contract
- [FFI contract](./ffi-contract.md): UniFFI surface (contract v2) and error mapping
- [Code standards](./code-standards.md): conventions and quality gates
- [ADRs](./adr/): decisions 001-012
- [Codebase summary](./codebase-summary.md): map of the repository
