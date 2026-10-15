# Tailscale vs pure LAN throughput (same LAN), 2026-10-15

## Setup
- Mac (alexpro): Wi-Fi 802.11ax, 192.168.1.239 / Tailscale 100.66.219.107.
- WASO (Windows): Ethernet 1 Gbps, 192.168.1.43 / Tailscale 100.127.164.17.
- Tailscale path: `active; direct 192.168.1.43:41641`, so WireGuard runs over the same LAN with no DERP relay.
- Tool: raw single-stream TCP (`tput.py`). The server ran on the Mac and the client on WASO.
- Each run moved 512 MiB. There were 3 rounds, with LAN and Tailscale runs interleaved.
- "up" means Windows→Mac and "down" means Mac→Windows.

## Results (MiB/s)
| Direction | LAN r1/r2/r3 | LAN avg | Tailscale r1/r2/r3 | TS avg | TS vs LAN |
|---|---|---|---|---|---|
| Win→Mac | 78.4 / 78.1 / 82.4 | **79.6** (~668 Mbit/s) | 52.0 / 50.9 / 54.9 | **52.6** (~441 Mbit/s) | −34% |
| Mac→Win | 66.1 / 68.6 / 69.1 | **67.9** (~570 Mbit/s) | 28.3 / 30.7 / 30.7 | **29.9** (~251 Mbit/s) | −56% |

- Ping over 20 packets, averaged: LAN 21 ms vs Tailscale 19 ms. The Wi-Fi is noisy (min 2–3 ms, max 92–130 ms), so latency is effectively the same.
- The spread between runs is small (<5%), so the gap is real.

## Interpretation
- The Wi-Fi link is the ceiling for pure LAN (~570–670 Mbit/s).
- Tailscale adds WireGuard encryption in userspace plus a smaller MTU (1280). The cost is worst when the Mac sends (−56%).
- fsync over Tailscale stacks two layers: QUIC/TLS 1.3 inside WireGuard. The earlier fsync-level runs showed the same pattern (Tailscale path 29–52 MiB/s vs LAN 40–64 MiB/s).

## Security
- fsync already encrypts end to end with QUIC/TLS 1.3, using the iroh node keys pinned at pairing. Pure LAN is just as confidential and authenticated as Tailscale.
- Tailscale adds only a redundant second encryption layer. It gives no extra protection for fsync data.

## Recommendation
- The core should prefer LAN paths, use any other direct path (Tailscale or another VPN) only when LAN is unreachable, and use the relay last. Final design (owner decision): generic interface rule, no Tailscale address ranges. This needs a custom iroh `PathSelector` (the `unstable-custom-transports` feature, pin `iroh = "=1.3.0"`).

## Unresolved questions
- Measured on Wi-Fi only. On wired↔wired the gap could be larger (CPU-bound) or smaller.
- No fsync-level bench this round. The dev backend was removed, and today's core can't force a path.
