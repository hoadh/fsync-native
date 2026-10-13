# ADR 007 — iroh 1.x discovery, relay, path visibility, file/1 backend
Status: accepted (single-host spike; 2-machine rows still open, see "Not measured")

Spike: `spikes/iroh-discovery` (iroh 1.3.0, iroh-relay 1.3.0, iroh-blobs 0.103.1, iroh-mdns-address-lookup 0.6.0, noq 1.3.0). Host: macOS arm64, one machine running relay + 2 processes, many interfaces (en0 192.168.1.239, Tailscale, IPv6). Numbers are real output of that host only.

## Decision
1. Endpoint: `presets::Minimal` (no n0 relay, no n0 DNS/pkarr) + `RelayMode::custom([our relay])`.
2. Address resolution: `MemoryLookup` (or own `AddressLookup`) fed from backend presence (relay URL + direct addrs) + `iroh-mdns-address-lookup` for LAN. Own `mdns-sd` only if 2-machine LAN tests miss <2 s.
3. Accept loop: `iroh::protocol::Router` with one `ProtocolHandler` per ALPN (control/clip/file).
4. UI path state from `Connection::path_events()` / `paths()` (`is_ip()` = direct/LAN, `is_relay()` = relay).
5. file/1: use `iroh-blobs` 0.103 (hash-addressed by blake3 from our index) behind a thin `FileTransfer` trait so it can be swapped for own `bao-tree` protocol (+3 d, phase 07) if API churn or store-copy cost bites.

## Go / No-Go per spec row
| Check | Result | Evidence |
|---|---|---|
| LAN discovery w/o internet (<2 s) | GO with caveat | mDNS id-only dial worked 10/10, but 396-3830 ms, median ~2.7 s, 4/10 under 2 s on this multi-interface host. Needs re-measure on 2 machines. Fallback `mdns-sd` kept as option. Backend-fed addrs: 1 ms. |
| Custom relay only | GO | Minimal preset + `RelayMode::custom`; relay-only endpoints (`clear_ip_transports`) connect in 2-3 ms, never go direct, 256 MiB over relay 117 MiB/s |
| Path type visibility | GO | `paths()`, `path_events()`, `Path::is_ip/is_relay/is_selected` |
| Hole punch, 2 NATs | NOT MEASURED, needs 2 machines | Only relay->direct upgrade on same host seen: 5-27 ms |
| iroh-blobs on 1.x | GO with risks | Depends on iroh 1.0.0, works on 1.3. Kill -9 mid-fetch, resume fetched only missing bytes, hash verified |

## Measurements (single Mac)
| Scenario | Result |
|---|---|
| mDNS only, dial by id, relay configured (6 runs) | 2994, 3627, 396, 840, 1388, 2345 ms |
| mDNS only, no relay (4 runs) | 3037, 3830, 840, 3102 ms |
| Control: no discovery, id only | `NoAddress` error (as expected) |
| Static lookup relay+ip, id-only dial | connect 1 ms, direct selected 4-8 ms, RTT ~0.2-0.3 ms |
| Static lookup relay URL only | connect 2-3 ms, relay->direct 5-27 ms |
| `ep.online()` with IP transport vs http-only dev relay | ~3.0 s (net_report QAD probe timeout); `--no-ip`: ~0.2 s |
| Relay-only 256 MiB | 2.19 s = 116.8 MiB/s; 2.17 s = 117.8 MiB/s |
| Direct 1 GiB via 192.168.1.239 | 14.9-16.5 s = 62-69 MiB/s (~520-576 Mbit/s) |
| Direct 1 GiB/512 MiB via 127.0.0.1 | 143.6 MiB/s / 138.1 MiB/s |
| iroh-blobs 200 MiB add (FsStore) | 179 ms |
| iroh-blobs fetch loopback | ~120 MiB/s |
| iroh-blobs kill/resume | killed at 144,670,720 B local; resume fetched ~65,044,480 B in 534 ms; blake3 of export == source hash; `cmp` identical |

Caveats: throughput is host-CPU bound (userspace QUIC, MTU 1452 on en0 path vs 16384 on lo0); relay over loopback beat the en0 self-path. NOT a % of real link. Target ">= 80% link" is unverified. Dial time includes bind-to-handshake with discovery. Dial opens several IP paths in ms (tailscale/LAN/v6) then closes losers.

## Not measured (needs 2 machines)
- Multi-machine LAN discovery latency and %-of-link throughput
- NAT hole punch across 2 NATs (home + hotspot); relay/direct ratio
- Windows firewall behavior for mDNS/UDP (installer rule note for phase 16)
- Relay with TLS + QUIC (prod config); `online()` delay there is likely much shorter, untested
- Internet-unplugged run (Minimal preset uses no n0 services, so no dependency observed, but Mac stayed online)

## file/1 backend assessment
iroh-blobs 0.103.1 fits: pull by hash, `store.remote().fetch(conn, HashAndFormat::raw(hash))` fetches only locally-missing ranges (= resume, chunk granular, 1 KiB, bao-tree 0.16, survives SIGKILL), every chunk verified. Provider side `EventSender` can intercept/reject requests (authz hook against trusted-device list).
Gaps:
- Addressing is by blake3 hash only; no Rep/File id lookup. App maps file id -> hash via our index (already blake3).
- Own store (`<hash>.data` + `.obao4` outboard ~0.4% + sqlite-like db). `ImportMode::Copy` default => 2x disk until export; `TryReference` may be ignored by store.
- 0.x API churn; pin exact version.
- Offset-resume of a specific byte range is expressed as `ChunkRanges` (`GetRequest::blob_ranges`), 1 KiB granularity.
Custom protocol on plain bi-streams + blake3: trivial for send/recv, but verified streaming + resume need `bao-tree` outboards and range bookkeeping (own partial state, crash-safe) = the +3 d in spec, plus ongoing maintenance. Whole-file blake3 verify after receive alone gives no early corruption detect and no verified resume. Recommend iroh-blobs first, fallback own file/1.

## Exact API (iroh 1.3)
Renames vs older iroh: Discovery -> `AddressLookup`, NodeId -> `EndpointId`, NodeAddr -> `EndpointAddr`.

Endpoint with custom relay:
```rust
use iroh::{Endpoint, SecretKey, RelayMode, RelayUrl, endpoint::presets};
let ep = Endpoint::builder(presets::Minimal)
    .secret_key(SecretKey::generate())            // or SecretKey::from_bytes(&[u8;32])
    .alpns(vec![b"spike/0".to_vec()])
    .relay_mode(RelayMode::custom([relay_url]))   // RelayUrl
    // .clear_ip_transports()  => relay-only endpoint
    // .address_lookup(builder) / .clear_address_lookup()
    .bind().await?;
ep.id();  ep.addr();  ep.online().await;  ep.close().await;
```
Static / backend-fed lookup (`iroh::address_lookup::MemoryLookup`, Clone):
```rust
let lookup = MemoryLookup::new();
let ep = Endpoint::builder(presets::Minimal)/*..*/.address_lookup(lookup.clone()).bind().await?;
// on each backend presence update:
lookup.add_endpoint_info(EndpointAddr::new(peer_id)
    .with_relay_url(relay_url).with_ip_addr(sock_addr));   // also set_/remove_endpoint_info
let conn = ep.connect(EndpointAddr::new(peer_id), b"spike/0").await?; // resolves via lookups
```
Custom provider: implement `iroh::address_lookup::AddressLookup` (`publish(&self,&EndpointData)`, `resolve(&self, EndpointId) -> Option<BoxStream<Result<Item,Error>>>`; `Item::new(EndpointInfo, provenance, last_updated)`); any impl is accepted by `.address_lookup()`. Add after bind: `ep.address_lookup()?.add(x)`. `TransportAddr::{Relay, Ip, Custom}`. Simplest for phase 06: keep a `MemoryLookup`, push from presence stream.

Local-network discovery (separate crate, not in iroh core):
```rust
use iroh_mdns_address_lookup::{MdnsAddressLookup, DiscoveryEvent};
let mdns = MdnsAddressLookup::builder().advertise(true)
    .service_name("fsync")                // default "irohv1"
    .build(ep.id())?;                     // builder also takes .addr_filter(AddrFilter::ip_only())
ep.address_lookup()?.add(mdns.clone());   // or .address_lookup(builder) at bind
let events = mdns.subscribe().await;      // DiscoveryEvent::{Discovered{endpoint_info,..}, Expired{endpoint_id}}
```
Accept with ALPN routing:
```rust
use iroh::protocol::{Router, ProtocolHandler, AcceptError};
impl ProtocolHandler for H { async fn accept(&self, conn: Connection) -> Result<(), AcceptError> { .. } }
let router = Router::builder(ep).accept(b"spike/0", H).accept(iroh_blobs::ALPN, BlobsProtocol::new(&store, None)).spawn();
router.shutdown().await?;   // AcceptError::from_err(e) to wrap errors
```
(manual: `ep.accept().await -> Option<Incoming>` -> `.accept()` -> `alpn()` -> `.await`.)

Remote id from incoming connection: `conn.remote_id()` (EndpointId from TLS cert, also `conn.alpn()`). Use it to check trust list in `accept`.

Path watcher:
```rust
let mut ev = conn.path_events();   // owned PathEventStream, spawnable
// PathEvent::{Opened{id,remote_addr,local_addr}, Closed{..,last_stats}, Selected{id,remote_addr,local_addr}, Lagged{missed}} non_exhaustive
for p in conn.paths().iter() { p.is_selected(); p.is_ip(); p.is_relay(); p.rtt(); p.remote_addr(); p.stats(); }
// conn.paths_stream() = snapshots, borrows conn
```
Direct vs relay = selected path `is_ip()` vs `is_relay()`. Streams: `conn.open_bi()/accept_bi()`, `send.write_all/finish`, `recv.read/read_exact` (noq types).

In-process relay (tests, dev): `iroh_relay::server::{Server, ServerConfig, RelayConfig}`; `cfg.relay = Some(RelayConfig::new(([127,0,0,1], port)))` (tls None => http, no QUIC); `Server::spawn(cfg).await`. Prod needs TlsConfig + QuicConfig (phase 10). Tests only: `.ca_tls_config(CaTlsConfig::insecure_skip_verify())`.

iroh-blobs:
```rust
let store = FsStore::load(dir).await?;
let tag = store.blobs().add_path(path).await?;               // tag.hash, tag.format
store.blobs().export(hash, out_path).await?;
let info = store.remote().local(HashAndFormat::raw(hash)).await?; // local_bytes(), is_complete()
let mut p = store.remote().fetch(conn, HashAndFormat::raw(hash)).stream(); // GetProgressItem::{Progress(u64),Done(Stats),Error(GetError)}
// store.downloader(&ep).download(hash, providers); GetRequest::blob_ranges(hash, ChunkRanges)
```

## Operational notes
- `online()` can take ~3 s against http-only relay; do not block UI or dials on it. Dialing with relay hint works without `online()`.
- mDNS: expect up to ~4 s on multi-interface hosts; run backend-fed addrs in parallel, race both.
- Open: iroh-mdns knobs not exposed (probe interval); revisit with `mdns-sd` if real LAN >2 s.

## Addendum (phase 07): file/1 backend decision
Decision: own `fsync/file/1` ALPN with a flattened chunk-group outboard built on `blake3::hazmat`; `iroh-blobs` / `bao-tree` are NOT used in `fsync-core`.

Design: files are cut into 16 KiB groups (`CHUNK_GROUP`). The sender keeps the non-root chaining value (CV, 32 B) of every group (0.2 % of file size). A fetch is `FetchReq{item,target,offset,len,no_cvs}` -> `FetchResp{status,size,hash}` -> CV list -> raw data. The receiver merges the CV list with blake3's tree rule and requires it to equal the hash announced in the (already validated) offer, then verifies each group against its CV before writing it. This gives the same guarantee as a bao outboard (per-chunk verification, out-of-order/ranged verification) in ~150 lines with no tree-state files.

Rationale (vs the ADR's iroh-blobs recommendation):
- Authorization and lookup are by `(item_id, Rep|File id)` in the sender's outbox; requester must be in the pinned device list AND offered the item. iroh-blobs is hash-addressed, so a second authz layer (event hooks) plus an id->hash map would be needed anyway.
- No second copy of user files: the sender serves the original path (size + mtime re-checked per fetch -> `Changed`), iroh-blobs `Copy` import would double disk use.
- The receiver store is a plain content-addressed directory behind the `BlobStore` trait, so phase 08 can wrap it with encryption at rest; iroh-blobs' store is opaque.
- Resume: `<hash>.part` only ever holds verified bytes written sequentially; on resume the kept prefix is re-verified against fresh CVs (crash-safe), then fetching continues at the next group boundary. Test: abort at ~6 MiB of 64 MiB, resume received exactly size - resumed_from.
- Ranged reads (`read_file_range`) reuse the same session with `offset/len`, so Windows virtual-file reads need no blob import.
- Tamper test hook (`EngineConfig::serve_hook`) proves a flipped byte yields `TransferError::Integrity` and never a complete blob.

Swap path: everything network-facing sits behind `file::fetch_to_store(dialer, store, peer, item, target, expect)` and `BlobStore`; replacing it with iroh-blobs only touches `fsync-core/src/file/`. Cost accepted: we own ~1.2 kLOC of transfer code (no tree-structured partial state: only prefix resume, no sparse multi-range partials).
Measured: 1 GiB loopback 123 MiB/s (vs 120 MiB/s for iroh-blobs in the spike), see `docs/protocol-spec.md`.
