# Mac <-> Windows LAN transfer verification (plan S7)

Scope: testing only, no repo source edits. Build: fsync-cli + fsync-backend @ 2cfeba5 (Mac), fsync-cli @ 2cfeba5 detached (Windows `fsync-pairtest`).
Backend: dev mode, `0.0.0.0:18080`, PG 17 container (removed). Relays: `relay-fsync.10z.one` (18.223.244.168), `relay-sg-fsync.10z.one` (52.77.206.192). Mac en0 is **Wi-Fi**. Windows Ethernet 2 is 1 Gbps, profile **Public**.

## Verdict

- Bulk data does go direct (no relay) between the two machines, and it physically crosses the LAN.
- The "LAN only, no Tailscale" criterion FAILS for 3 of 8 direct runs. Bulk went through the Tailscale interface (utun4 / Tailscale adapter), carried as WireGuard on 192.168.1.43:41641.
- Pure LAN is proven by pcap in 2 of 2 captured 64 MiB runs (IPv4 192.168.1.x). The 256 MiB W->M run was pure LAN over IPv6 on-link (same /64).
- Cause: iroh advertises all local addresses (IPv4 LAN, IPv6 global, Tailscale 100.x). CLI does not filter, and `via: Lan` is shown for every direct IP path. The selected address is not exposed.

## Results

| # | Case | Dir | Result | Key numbers (send / recv MiB/s) | Path evidence |
|---|------|-----|--------|---------------------------------|---------------|
| 1 | Login | M, W | PASS | Mac `created`, Win `needs_pairing` | |
| 1 | Pair (Win offer, Mac join) | W->M | PASS | SAS `nobe reba fezi` on both | |
| 2 | Clip text | M->W x3 | PASS 3/3 | est. one-way 50-135 ms | direct |
| 2 | Clip text | W->M x3 (+1 dup run) | PASS 4/4 | est. one-way 45-142 ms | direct |
| 3 | Bench 256 MiB, run A | M->W | PASS (fetch OK), **FAIL LAN** | 31.4 send / recv not recorded | **Tailscale**: utun4 +289 MB, Tailscale adapter +283 MB |
| 3 | Bench 256 MiB, run A | W->M | PASS, **FAIL LAN** | 51.7 / 38.6 | **Tailscale**: utun4 +283 MB in, Tailscale +283 MB out |
| 3 | Bench 256 MiB, run B (pcap) | M->W | PASS, mixed | 38.6 / 30.1 | IPv6 on-link 94 MB + Tailscale WG over LAN 202 MB (utun4 +195 MB) |
| 3 | Bench 256 MiB, run B (pcap) | W->M | PASS, LAN | 64.2 / 44.9 | IPv6 on-link 287 MB; Tailscale 24 KB |
| 4 | Files 64 MiB random, SHA256 | M->W, run A | PASS (hash OK), **FAIL LAN** | 36.5 / 28.8 | Tailscale: utun4 +75 MB |
| 4 | Files 64 MiB random, SHA256 | W->M, run A | PASS (hash OK), LAN | 58.0 / 42.4 | Ethernet 2 +71 MB; Tailscale 18 KB (IP not captured) |
| 4 | Files 64 MiB random, SHA256 | M->W, run C (pcap) | PASS (hash OK), LAN | 53.2 / 40.2 | IPv4 192.168.1.239 <-> .43, 70.8 MB; WG 0.09 MB |
| 4 | Files 64 MiB random, SHA256 | W->M, run C (pcap) | PASS (hash OK), LAN | 58.0 / 42.3 | IPv4 192.168.1.43 <-> .239, 70.8 MB; Tailscale 25 KB |
| 5 | Control `--force-relay` 64 MiB | M->W | PASS (hash OK), Relay | 9.47 / 9.49 | relay-sg TCP 443: 74.5 MB; en0 +75 MB; Ethernet 2 +74.5 MB; no direct bulk |
| 6 | Windows Public profile | M->W | Direct works, ICMP blocked | ping fails, all UDP paths work | see below |

Counter deltas (bytes, Mac en0 / utun4; Win Ethernet 2 / Tailscale) are in `$S/lan-test/bench-*` and `files-*` dirs.

Expected payload is 268 MiB (281.5 MB) or 64 MiB (67.1 MB). Wire bytes include ~3-12% overhead.

### Path detail
- Tailscale status during run A: `waso ... direct 192.168.1.43:41641`. So the tailnet payload rode WireGuard over the LAN, not a relay.
- pcap (`pktmon`, Windows NIC, 96-byte snap): no 100.x or relay bulk in the direct LAN runs. Relay IPs appear only in the forced-relay run.
- Mac-side per-address capture not possible (no sudo). Mac side is counters only.

### Windows Public profile (case 6)
- Direct data flows work on Public. Mac->Windows ICMP times out.
- Public default inbound = NotConfigured (default block). Allow rules exist for `fsync-cli.exe` only at the `fsync-native` worktree path, not `fsync-pairtest`. Tailscale rule is `Any` profile for `tailscaled.exe`.
- Inference: direct succeeds through stateful UDP hole punching (Windows-initiated flows). Unsolicited first packets from the Mac were not tested.
- No change recommended. If the owner wants a firewall-independent guarantee, set Ethernet 2 to Private. Not done here.

### Clip latency method
- `put` and `watch` clocks are on different machines. Windows clock is ~2.83 s ahead of Mac (estimated from the pairs; ssh-based estimates of ~3.2 s were unreliable).
- Per pair, raw M->W + raw W->M = sum of two one-way latencies, which is clock-independent: 277, 244, 95 ms.
- Per-direction numbers above assume symmetric split. Treat as approximate.

## Deviations from spec
- CLI has `send` / `fetch` (not `files push/recv`). `--insecure-http` is the CLI flag. The backend has no `--allow-http`.
- W->M clip runs: round 3 was run twice by mistake. Both passed, the second overwrote the first's files. Reported as 4 runs.
- Relay control used 64 MiB (not 256) to limit time. Relay throughput was ~9.5 MiB/s, far above the ~0.5 MiB/s in older notes.
- No Mac-side pcap, no sudo. Mac capture is counters only.

## Clip sanity
- Four W->M clip runs measured with raw sums under 0.3 s round trip. No clip run exceeded 1 s. Only 4 samples, so no conclusion on rare delays.

## Cleanup
- Stopped backend (Mac), removed container `fsync-lan-pg-1015`. `mystifying_euclid` untouched. No Docker prune.
- Closed tmux windows `lan-*`. Session `main` untouched.
- Windows: no fsync-cli processes, `pktmon` stopped, large payloads and ETL/pcap removed. Worktree clean at 2cfeba5. Kept `C:\Users\dev\lan-test-1015\data` (test account state) and small helper ps1 scripts.
- Mac: scratch in `$S/lan-test` (logs, pcaps `cap.pcap`, `cap2.pcap`, counters, `mac-data`). Repo `git status` unchanged (only `.claude/` untracked, pre-existing).
- Tailscale left up. Windows firewall and profiles unchanged.

## Recommendations
1. Core: expose the selected remote address in CLI JSON (for example in `send`/`clip` events, or `devices --json`). Right now "Lan" hides the path.
2. Core (owner decision): add a candidate filter or flag for LAN-only testing (exclude CGNAT 100.64/10 and tailnet interfaces, optionally IPv6). Without it, LAN-only cannot be proven while Tailscale is up.
3. Core: log iroh path changes (selected path changes) in CLI tracing. Path flips between runs (run A used Tailscale, run C used IPv4 LAN for the same direction).
4. Test harness: add a pktmon/tcpdump step to the e2e script so path evidence is automatic.
5. Owner: decide whether the Windows Public profile is acceptable. Direct works today. Add an inbound allow rule for the real fsync-cli binary if strict behavior is needed.

## Unresolved questions
1. Why does iroh choose Tailscale (100.x), IPv6 on-link, or IPv4 LAN per run? Same direction gave different paths in different runs (W->M 256: Tailscale in run A, IPv6 in run B). Needs path-selection logs.
2. Is the Tailscale-candidate path acceptable product behaviour, or must transfers be pinned to physical LAN? Owner decision.
3. Does an unsolicited first packet from the Mac get through the Public profile? Not tested. Would need a listener, and I did not change firewall rules.
4. Exact IP used in run A W->M 64 MiB (IPv4 or IPv6) was not captured.
5. Windows clock offset is only estimated (~2.83 s). Enabling time sync would make latency exact.
6. Mac-side per-address capture needs root. Not done.

**Status:** DONE_WITH_CONCERNS
**Summary:** Pairing, clip (3+4 runs), bench 256 MiB both ways, and 64 MiB files with SHA256 all pass, direct and forced relay. Direct bulk is physically on LAN, but in 3 of 8 direct runs it went through the Tailscale interface (WireGuard over LAN). Pure LAN is pcap-proven for 64 MiB M->W and W->M (IPv4) and 256 MiB W->M (IPv6 on-link). Forced relay ~9.5 MiB/s. Direct works with Windows Public profile.
**Concerns/Blockers:** The CLI cannot show which address carried the bytes, and iroh picks tailnet candidates in some runs, so "LAN-only" is not guaranteed. Needs owner decision and a CLI change to pin or report the path. Full list of unresolved questions above.
