# Phase 06 — Core: identity, signed device list, transport, pairing

## Context Links
- Design §2 Identity, §4 flows 1-2; [phase-02](phase-02-spike-iroh-discovery-relay.md) ADR 007
- Backend counterpart: [phase-09](phase-09-backend-auth-registry-presence.md)

## Overview
- Priority: P0 | Status: Partial | Effort: 2 wk (A)
- Device keypair, `DeviceList` chain (create/verify/sign), iroh endpoint with accept-gate (only listed NodeIds), discovery, path-state tracking, pair/1 protocol.

## Key Insights
- Device key = iroh ed25519 secret key; one key for transport identity + list signing (KISS). Stored by app in Keychain/DPAPI, passed to core at start (core never writes raw key to disk itself on mac/win; CLI uses file w/ 0600).
- Server stores list but cannot forge (no device keys). Server CAN withhold/rollback → clients pin highest `seq` + hash seen and reject lower.
- 6-digit code is low-entropy → must use PAKE (SPAKE2) so a malicious backend relaying the code→NodeId lookup cannot MITM. QR carries 128-bit secret → MAC confirm only.

## Requirements
- Reject any incoming connection whose remote NodeId ∉ current verified list (except during active pairing window on `fsync/pair/1`).
- Device removal propagates on next list fetch/presence push; removed device connections closed immediately.

## Architecture
### DeviceList (fsync-proto, postcard canonical)
```
struct DeviceList {
  v: u8 = 1, account_id: Uuid, seq: u64, prev_hash: [u8;32] /*blake3(prev SignedDeviceList bytes), zero at genesis*/,
  devices: Vec<DeviceEntry{ node_id: [u8;32], name: String, platform: Mac|Win|Cli, added_seq: u64 }>,
  signer: [u8;32], created_ms: u64 }
struct SignedDeviceList { body: Vec<u8> /*postcard(DeviceList)*/, sig: [u8;64] /*ed25519(signer, "fsync-devlist-v1" || body)*/ }
verify(new, pinned):
  sig valid for body.signer; body.signer ∈ pinned.devices (or genesis: seq==0 && signer is sole device)
  new.seq == pinned.seq+1 && new.prev_hash == blake3(pinned bytes)   // or fetch & verify chain gap
  account_id matches; reject seq <= pinned.seq unless identical bytes
re-genesis (all devices lost): new seq-0 chain → old devices see account_id same + root changed → "Trust reset" warning, require user confirm.
```
### pair/1 handshake (ALPN `fsync/pair/1`, length-prefixed postcard frames)
```
New device N (logged in): pairing_offer() → POST /pairing {node_id_N, relay_url} → backend returns code (6 digits, 5 min, single use)
  QR payload = "fsync://pair?v=1&n=<node_id_N b32>&s=<secret128 b64>&r=<relay>"
Existing device E: code → GET /pairing/{code} → node_id_N   |  QR → node_id_N + secret
E dials N on pair/1:
  E→N Hello{v:1, mode: Code|Qr, node_id_E}
  Code: SPAKE2 (pw = code, ids = node_id_E||node_id_N) E→N Spake{msg}, N→E Spake{msg} → K
  QR:   K = HKDF(secret, node_id_E||node_id_N)
  both: Confirm{mac = HMAC(K, transcript)} ; mismatch → Abort, code burned
  N→E DeviceInfo{name, platform}
  E UI: "Add <name> (<platform>)? SAS: 3 words from K" → user accepts
  E: list' = list + N, seq+1, sign → PUT /device-list (If-Match seq) → E→N SignedList{full chain or latest + pinned genesis}
  N verifies, pins, Done. Rate limit: 5 attempts/code (backend) + abort on first bad MAC.
```
### Engine modules (each < 200 lines)
```
core/fsync-core/src/
  identity.rs        # key load/generate, NodeId helpers
  trust/device_list.rs, trust/verify.rs, trust/store.rs  # pinning in SQLite (phase 08 db)
  net/endpoint.rs    # iroh Endpoint builder, ALPN router, accept gate
  net/discovery.rs   # backend-presence discovery + local-network (ADR 007)
  net/path_state.rs  # per-peer Offline|Lan|Relay watcher → delegate
  pair/protocol.rs, pair/spake.rs, pair/qr.rs
  api_client.rs      # reqwest: device-list, pairing, presence WS
```
Data flow: backend presence/WS → discovery addrs → iroh dial → accept gate (trust store) → protocol handlers (phase 07).

## Related Code Files
- Create: `core/fsync-proto/src/{device_list.rs, pairing.rs, api_dto.rs}`, files above
- Modify: `core/fsync-core/Cargo.toml`

## Implementation Steps
1. fsync-proto types + canonical encode + sign/verify + unit tests (forged sig, rollback, gap, wrong account).
2. Endpoint + accept gate + discovery per ADR 007.
3. Path state watcher → events.
4. pair/1 with SPAKE2 (`spake2` crate) + QR mode; SAS words from K.
5. api_client for device list + pairing (against phase 09 contract; until ready use local axum test stub in tests only).

## Todo List
- [ ] DeviceList types + verify (100% branch tests)
- [x] Endpoint + accept gate
- [x] Discovery + path state
- [x] pair/1 code + QR modes
- [x] api_client

## Implementation Notes (sync 2026-10-14)
- Done: DeviceList sign/verify (fsync-proto), accept gate, path state, SPAKE2 code + QR modes with mandatory SAS on both sides, api_client, presence. Tests include real-backend `e2e_api` and `e2e_pair_flow`.
- Hardening from reviews 06/07 and 08b: consent before pin (C1), pairing window guard, `pairing_cancel`, re-pair after reset via `listed_head` (round 7, `pair/initiator_steps.rs`). Round-6 review found no SAS/MAC bypass.
- Deviation: SignedList carries head only, not full chain. Root stays `UNKNOWN_ROOT` until a chain through the head is seen. Replay of an old genesis then prompts a reset.
- Open: backend keeps last 100 rows per epoch, so a fresh device cannot always get a chain from genesis. Checkpoint mechanism not built.
- Done 2026-10-14: TrustReset confirm API (M5) in trust manager, service, FFI and CLI (ADR 012). Pending/declined state is in memory only. No new todo ticked: the phase todos above are unchanged.
- Not measured: "100% branch" coverage (tests exist, branch coverage not run); two-machine code and QR pairing.
- SAS is 3 CVCV words (36 bits). Hole-punched direct paths report as `Lan`.

## Success Criteria
- CLI (phase 08) pairs 2 machines via code and QR; unpaired NodeId connection rejected (test); rollback list rejected (test).

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Concurrent list updates from 2 devices | L x M | If-Match seq on PUT; loser refetches + reapplies |
| Backend withholds removal (freeze) | L x M | removal also pushed peer-to-peer to online devices |
| Key loss on OS reinstall | M x L | re-pair; documented |

## Rollback
- Versioned (`v`, ALPN); new formats = v2 coexisting. Revert phase = revert crate commits; no persistent prod data yet.

## Security Considerations
- Domain-separated signatures; constant-time MAC compare; codes single-use + 5 min TTL; keys never logged; zeroize secrets.

## Next Steps
- Phase 07 protocols use accept gate + path state.
