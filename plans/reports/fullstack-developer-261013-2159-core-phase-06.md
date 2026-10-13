# Phase 06 fsync-core: public API

Crate `core/fsync-core`. iroh 1.3, spake2 0.4, reqwest 0.12 + tokio-tungstenite 0.28 (rustls, ring provider only). All src files < 200 lines. No unwrap/expect in src.

## Root re-exports
`CoreError, ApiError{status,code,message; is(code)}, PairError, Result, EngineEvent, EventSink, PairDecision, PairingEvent, PathState, Identity, DeviceInfo, ListBackend, PairSecret`. Modules: `api, error, events, identity, net, pair, sync, trust`.

## Errors
`CoreError{Proto, Pair(PairError), Api(ApiError), Io, Codec, Net(String), Http, Ws, Store, NotTrusted(String), AuthExpired, Invalid(&str), Closed}`.
`PairError{BadMac, Rejected, PeerAborted(AbortReason), Timeout, NoWindow, NoPinnedList, Protocol(&str)}` (Clone; preserved across the window result so callers can match `BadMac`).

## Identity
`Identity::{generate, from_seed(&[u8;32]), load_or_create(path) (0600), seed() -> Zeroizing, signing_key(), node_id(), iroh_secret()}`; Clone; Debug redacted.

## Events (`EventSink::channel() -> (sink, mpsc::Receiver<EngineEvent>)`, buffer 256, try_send)
- `EngineEvent::{DeviceStatus{node_id,state:PathState}, ListAdvanced{seq}, Pairing(PairingEvent), TrustReset{root_hash,devices}, Error{message}}`
- `PathState::{Offline, Lan, Relay}` (Lan = any direct IP path incl. hole-punched)
- `PairingEvent::{SasShown{sas}, ConfirmRequest{peer,name,platform,mode,sas,decision:PairDecision}, Completed{peer,seq}, Failed{reason}}`; `PairDecision::{accept(), reject()}`.

## Trust
- `trait TrustStore{load,save,clear}`; `FileTrustStore::new(path)` (atomic, 0600), `MemoryTrustStore`; `PinnedState{head,root_hash}`.
- `TrustManager::new(Arc<dyn TrustStore>, EventSink)`; `pin_genesis(&Signed)`, `pin_paired_chain(chain, me, peer) -> seq`, `ingest(&Signed) / ingest_chain(&[Signed]) -> Ingest{Unchanged, Advanced{seq}, ResetPending{root_hash}}`, `confirm_reset(&root_hash)`, `pending_resets()`, `head()`, `list()`, `root_hash()`, `is_trusted(&NodeId)` (fail-closed), `subscribe() -> watch::Receiver<u64>`, `clear()`.
- Rollback (older seq) ignored/rejected; seq-0 with different root = reset, prompts once per root, never auto-accepted; a replay of an old genesis is not a reset.

## Net
- `Node::builder(identity, trust, events) -> NodeBuilder{relays(Vec<String>), mdns(bool), protocol(alpn, handler), spawn()}`. No n0 defaults; empty relays = RelayMode::Disabled.
- `Node{discovery: Discovery, id(), endpoint(), local_addrs() -> Addrs, path_state(&peer), connect(&peer, alpn) (trusted only, tracked), await_pairing(secret, DeviceInfo, ttl) -> seq, pair_with(&peer, relay, direct, &secret, &impl ListBackend) -> seq, shutdown()}`.
- Accept gate (`Gate`, `Gated<H>`): remote id must be in pinned verified list, except ALPN fsync/pair/1 while exactly one pairing window is open (consumed by first session). Untrusted live conns closed with code 403 on every trust change (device removal).
- `Discovery{hint(node, relay, direct), apply_peer, apply_peers, refresh}`: MemoryLookup fed from presence for trusted peers only; optional mDNS ("fsync").
- `PathTracker`: per-peer best path across connections, emits DeviceStatus on change.

## Pairing (pair/1)
`PairSecret::{Code(Zeroizing<String>), Qr(Zeroizing<[u8;16]>)}`; `trait ListBackend{publish(list, prev_seq), sync(), full_chain()}`; `pair::{PairKeys, Role, qr_key, transcript, run_initiator, run_responder, PairingWindows}`.
Code: SPAKE2 (A=existing, B=new, ids=node ids); QR: HKDF(secret, salt=E||N). Both: MAC confirm (constant time, abort on first bad), SAS = 3 CVCV words (36 bits) shown on both, existing device must accept before signing. 409 on publish: up to 3 retries after `sync()`.

## API client (`api`)
- `ApiClient::new(base, Option<TokenPair>, Option<TokenCallback>)`; `tokens()`, `access_token()`, `refresh_now()`; single-flight refresh on 401, retry on 409 refresh_in_progress, refresh 401 -> tokens cleared + `AuthExpired`.
- `otp_start, otp_verify -> account Uuid, logout, get_device_list -> Option, put_device_list(signed, if_match), put_trust_reset, chain(from), chain_all(from), create_pairing(node, relay), lookup_pairing(code)`.
- `spawn_presence(api, identity, AddrsProvider, mpsc::Sender<PresenceEvent>) -> PresenceHandle` (abort on drop). Challenge->Auth, Hello, heartbeat 20 s, backoff 1..60 s + jitter, 4401 close -> refresh + reconnect. `PresenceEvent::{Connected, Disconnected, Server(ServerMsg), AuthExpired}`.
- `sync::{sync_device_list, ApiListBackend{api,trust}, run_presence_pump(rx, api, trust, discovery, events)}`. Pump ignores DeviceListChanged until a list is pinned.

## Proto/backend change
`ChainResponse` moved to `fsync_proto::api_dto`; backend imports it; backend tests updated. `cargo test -p fsync-backend` green.

## Tests (all run, all pass)
`cargo test -p fsync-core`: trust_manager (5), pair_kdf (4), net_gate_and_paths (5), pairing_iroh (7), e2e_api (2, real backend), e2e_pair_flow (1, real backend + Postgres 17 container). clippy `-D warnings --all-targets` clean, fmt clean.
Covered: rollback/forgery/gap, reset confirm + dedupe, file store 0600 + restart, SAS/QR KDF, MAC mismatch/reflection, unpaired rejected (raw + pair ALPN), removal closes conns, Lan on loopback, code + QR pairing with equal SAS, wrong code/QR secret -> BadMac + window burned, mode downgrade refused, user reject, no window refused, real-backend OTP login, genesis PUT, 409, chain, pairing create/lookup, token refresh callback, logout, presence auth, pair over iroh, DeviceListChanged push -> B pins seq 2.

## Concerns
- Backend prunes history to last 100 rows per epoch; a fresh device needs the chain from genesis (`verify_chain_from_genesis`). Pruned accounts cannot be paired as designed; needs checkpoint mechanism. Not verified in backend source.
- `PairingCreated.expires_at` is unix ms per proto; spec unstated.
- Self-removal not a separate event (gate just stops trusting).
- Direct hole-punched paths are reported as `Lan`.
- One attempt per pairing window; a first connector can burn it (DoS only, code mode stays safe).
- Code mode security depends on 36-bit SAS comparison vs a malicious backend.
- mDNS off by default; ADR 007 latency caveats stand. No relay-based test (loopback + MemoryLookup only).
- rustls provider fixed to ring (no aws-lc).

## Unresolved questions
- Checkpoint/prune strategy for chain from genesis (backend + proto).
- Should self-removal emit an EngineEvent in phase 08?
