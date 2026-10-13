# Phase 02 — Spike: iroh 1.x local discovery, relay, blobs

## Context Links
- Design §5 risk "iroh mDNS/local discovery status in 1.x unconfirmed"
- https://docs.rs/crate/iroh/1.0.3, https://docs.rs/crate/iroh-relay/1.0.0
- Blocks: [phase-06](phase-06-core-identity-trust-transport.md)

## Overview
- Priority: P0 | Status: Partial | Effort: 3-4 days (A)
- Prove iroh 1.x can: discover peers on LAN without internet, hole-punch across NAT, fall back to self-hosted relay, report path type (direct/relay) per connection. Evaluate `iroh-blobs` for file/1.

## Key Insights
- iroh dials by public key (NodeId/EndpointId — confirm 1.x name); address resolution via pluggable discovery. We must not depend on n0 public infra (DNS/pkarr, default relays) in prod.
- Path type observable via connection/conn-type watcher API — needed for UI "online-LAN / online-relay".
- `iroh-blobs` already implements blake3-verified resumable streaming (bao). Reuse if compatible with 1.x and stable; else own `bao-tree` based file/1.

## Requirements
- Measure: discovery time on LAN (target < 2 s), direct-path upgrade time, LAN throughput (target ≥ 80% link), relay RTT.
- Run with internet unplugged (LAN-only) and with `relay_mode = custom(our relay)`.

## Architecture
```
spikes/iroh-discovery/
  src/main.rs   # `listen` | `dial <node_id>` | `bench <node_id> <MiB>`
  # Endpoint::builder().secret_key(k).alpns([b"spike/0"])
  #   .relay_mode(Custom(local iroh-relay)).discovery(local_network + static)
```
Local iroh-relay via `iroh-relay --dev` on laptop.

## Go / No-Go
| Check | Go | Fallback if No-Go |
|---|---|---|
| LAN discovery w/o internet | peer found < 2 s via iroh local-network discovery | own mDNS (`mdns-sd` crate) publishing NodeId+addrs into custom `Discovery` |
| Custom relay | connects via self-hosted relay only | n/a — blocker; escalate (unlikely) |
| Path type visibility | API exposes direct vs relay | infer from remote addr; poll stats |
| Hole punch (2 NATs, home routers) | direct within 10 s in ≥ 1 of 2 test setups | accept relay; track ratio metric |
| iroh-blobs on 1.x | compatible + resumable range fetch | own file/1 over `bao-tree` (phase 07 +3 d) |

## Related Code Files
- Create: `spikes/iroh-discovery/**` (throwaway), `docs/adr/007-iroh-discovery-and-blobs.md` (result)

## Implementation Steps
1. Minimal endpoint, custom relay, print NodeId + conn type changes.
2. Two Macs/PC on same Wi-Fi, WAN off → test discovery.
3. Different networks (home + phone hotspot) → hole punch / relay.
4. Bench 1 GiB transfer LAN; record numbers.
5. Try `iroh-blobs` add/fetch with resume (kill mid-transfer, resume offset).
6. Write ADR 007 with numbers + chosen discovery stack.

## Todo List
- [x] Endpoint + custom relay
- [ ] LAN-only discovery test
- [ ] NAT/relay test
- [x] Throughput bench
- [x] iroh-blobs evaluation
- [x] ADR 007

## Implementation Notes (sync 2026-10-14)
- Done on one Mac (arm64): custom relay GO, path visibility GO, iroh-blobs resume GO (kill -9, 200 MiB, hash verified). ADR 007 written with numbers.
- Throughput, one host: relay loopback 117 MiB/s, direct en0 62-69 MiB/s, lo 144 MiB/s. Host-CPU bound.
- Miss: mDNS median ~2.7 s (4/10 under 2 s) vs <2 s target. mdns-sd fallback NOT built; deferred until 2-machine data.
- Deviation: iroh-blobs evaluated GO but file/1 uses own blake3 hazmat protocol (ADR 007 addendum; see phase 07).
- Not measured (needs 2 machines or real networks): LAN discovery with internet off, hole punch across 2 NATs, Windows firewall UDP, 1 GiB LAN bench, prod TLS relay (`ep.online()` ~3 s on HTTP dev relay).

## Success Criteria
- ADR 007 merged with measured numbers and go/no-go per row.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Local discovery missing/unstable | M x M | mdns-sd fallback (~150 LoC) |
| Windows firewall blocks UDP on LAN | M x M | note installer firewall rule requirement for phase 16 |

## Rollback
- Spike code isolated in `spikes/`; nothing to revert.

## Security Considerations
- Spike uses dev relay only; no real keys reused later.

## Next Steps
- Feeds phase 06 (discovery impl), 07 (file/1 backend choice), 10 (relay deploy config).
