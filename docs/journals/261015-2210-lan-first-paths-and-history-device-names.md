# 2026-10-15 22:10 — LAN-first paths and history device names

Owner wants LAN for speed, VPN only when no LAN path exists, and no Tailscale-specific logic. Mac history also showed raw node ids.

## What changed and why
- **Measured cost of the VPN**: same LAN, raw TCP over Tailscale was −34% (Win→Mac) and −56% (Mac→Win) vs LAN. That is why the owner asked for LAN preference.
- **Path selection**: custom iroh `PathSelector` ranks tiers Lan < Direct < Relay. Inside a tier it keeps iroh's biased RTT and 5 ms stickiness. The policy lives in a pure `choose()` so it can be tested without sockets.
- **LAN test is generic**: remote IP must sit inside a subnet of an up, non-loopback, non-p2p, non-tunnel/ppp interface that is `is_physical()` and has a gateway. No vendor names.
- **Subnet snapshot** refreshes on a background thread with `catch_unwind` and backoff. Blocking `get_interfaces` under a mutex inside `select()` was the first version and it was wrong.
- **Interface verdicts**: Mac `utun*` had `gw=true` but is p2p/Tunnel, so excluded. Windows Tailscale is `ProprietaryVirtual`, no gateway, `/32`. Only Wi-Fi (Mac) and "Ethernet 2" (Windows) counted as LAN.
- **Mac history**: row reads "From <device name>" (`PeerText.deviceName`, fallback "another device").
- iroh stays pinned to `=1.3.0` (`unstable-custom-transports`).

## Verification
- Live, Tailscale up: 256 MiB each way, SHA256 match, path `Lan`, 58/64 MiB/s. en0 / Ethernet 2 carried ~285 MB; Tailscale adapters carried ~45 KB.
- Signed Mac app: 8 history rows read "From WASO", 0 hex ids.
- Tests: cargo 321 passed / 0 failed; swift 231 (18 skipped) / 0 failures.
- Review rounds 7.5 → 9.0 → 9.4 → 9.7. Fixes along the way: background refresh, stricter overlay rule (physical + gateway), `choose()` extraction, a refresh-thread panic could freeze the cache, and reads that kept failing spawned a thread per call.
- The CLI has no tracing subscriber, so interface byte counters were the proof, not logs.

## Lessons
- Tailscale's gateway check alone is not enough on macOS: `utun` has `gw=true`. Stacking several generic signals worked; any single one did not.
- A wrong verdict only falls back to iroh's default ordering, so a heuristic was acceptable here. That would not hold for anything that blocks or rejects traffic.
- Measure with interface counters and a hash check. Trust neither the selector's logs nor "it feels faster".

## Open
- The "On LAN" UI label still means any direct path. It is misleading and needs a rename or a real LAN flag.
- Known limits: full-tunnel VPN that hijacks the default route, fe80 link-local, very wide prefixes.
- `select()` wiring has no unit test; only `choose()` is covered.
- The iroh pin at `=1.3.0` needs an upgrade plan before the next iroh release.
- The Windows pairtest clone holds copied uncommitted files. Clean it up before the next run.
