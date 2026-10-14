# Mac <-> Windows pairing, clip, file test (repo 85f8968)

Scope: testing only, no source edits. Windows worktree `C:\Users\dev\workspace\fsync-pairtest` (branch pairtest/cross-os, 85f8968). Fresh PG container, removed after.

## Environment caveats
- LAN 192.168.1.x unreachable between Mac (.27) and Windows (.43): ARP incomplete, ping/TCP timeout both ways. All runs over Tailscale (Mac 100.66.219.107, Windows 100.127.164.17).
- CLI path label "Lan" = any direct IP path (core/fsync-core/src/net/path_state.rs:connection_state), so tailnet-direct shows as "Lan". Path not verifiable from clip JSON.
- Relay: `iroh-discovery-spike` binds 127.0.0.1 only (spikes/iroh-discovery/src/main.rs:34). Relay runs used SSH reverse tunnel 13340.
- Windows Defender firewall on (no rules changed). Windows time service stopped, clock unsynced.
- Clock offset (win - mac) ~ -1.17 to -1.21 s, derived from symmetric one-way latencies. Latency numbers below are approximate.

## Test results

| # | Item | Result |
|---|------|--------|
| 1 | Login Mac (genesis) | PASS `account:created` |
| 1 | Login Windows | PASS `needs_pairing` |
| 1 | Pair Windows offer / Mac join, code + SAS | PASS, SAS `nobe kehe rili`, join wall ~1.2 s |
| 2 | Clip M->W direct | PASS, text match, raw delta -1130 ms |
| 2 | Clip M->W relay (`--force-relay`) | PASS, raw delta -1081 ms |
| 2 | Clip W->M relay | PASS, raw delta +1254 ms |
| 2 | Clip W->M direct | 2 FAIL then 2 PASS (see Failed items). Passes: +1233 ms, +1310 ms |
| 3 | File M->W direct 64 MiB | PASS, sha256 match, 1.30 MiB/s |
| 3 | File W->M direct 64 MiB | PASS, sha256 match, 4.7 MiB/s. Source was zero-filled (script bug, since fixed). Re-run with random data not done |
| 3 | File M->W relay 64 MiB | PASS, sha256 match, 1.38 MiB/s |
| 3 | File W->M relay 64 MiB | PASS, sha256 match, 3.6 MiB/s |
| 4 | Forced relay | Covered by relay rows above (clip both ways, file both ways) |
| 5 | Mac app integration (`apps/macos/scripts/run-fsync-mac-integration.sh`) | PASS, 3/3 (`testFirstDeviceJoinStatusAndRemove`, `testMacOffersAndCliJoins`, `testRecoveryResetsTrust`). Local stack with local CLI peer, not Windows |
| 5 | Windows app engine / UI | NOT RUN, needs interactive desktop (see manual steps) |
| 6 | Remove + re-pair | PASS. Mac `account reset-trust --yes`; Windows `trust resets` showed reset (devMac only), `trust confirm` -> `needs_pairing`; re-pair Windows offer / Mac join, SAS `zimo bire zipa`, `paired` both sides. Clips both directions pass after re-pair (M->W -1117 ms, W->M +1302 ms) |

Note: CLI has no per-device remove. Removal = account trust reset (core/fsync-cli/src/args.rs AccountCmd::ResetTrust).

## Failed items

### W->M direct clip, first attempt (intermittent)
- Windows put: `published`, `delivered` Mac node, 21:42:34.
- Mac `clip watch --count 1` (pid 34814) got no clip for its 120 s window and exited `error: timed out`.
- Mac file `clip-direct/watch.out` later contains the clip, `ts_ms` 21:44:06 (+92 s after publish). So the event was delayed, not dropped. Mac history also recorded the item.
- Unresolved: that watch printed the clip but still did not exit on `--count 1`. Repro watch (`clip-repro`) got nothing within its window.
- Hypothesis (unverified): `clip put` exits right after delivery (put.out 21:43); text payload is lazily fetched from origin, so the Mac receive depends on origin liveness and retries. Not confirmed.
- Not reproduced in 2 later direct runs (+1233, +1310 ms).

## Critical issues
- None blocking. Intermittent W->M text clip delay (92 s once, 2/4 direct attempts with a long wait) is the main sync risk. Owner: core (CLI watch/event path, lazy text fetch), needs tracing in CLI.

## Recommendations
1. core: add tracing subscriber to fsync-cli so "engine event dropped" (core/fsync-core/src/events.rs:145-149) and lazy-fetch retries are visible.
2. core: check `clip watch --count N` exit after the N-th clip (exit code vs timeout).
3. core/backend: make text clip fetch not depend on origin staying alive after `clip put` returns, or document the constraint.
4. core/spikes: iroh-discovery spike relay binds 127.0.0.1 hardcoded (spikes/iroh-discovery/src/main.rs:34). Make bind address configurable for LAN relay tests.
5. infra: fix LAN L2 reachability between Mac and Windows, then re-run direct paths on 192.168.1.x and verify path label.
6. Windows: enable time sync (`w32tm`, service stopped) so one-way latency is measured, not inferred.
7. Windows: scripts/tests need `C:\Users\dev\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin` on PATH (`.cargo\bin` proxies are 0-byte).
8. Test script: PowerShell 5.1 lacks `RandomNumberGenerator.Fill`; use `Random.NextBytes` (fixed in harness).
9. Perf: tailnet file throughput ~1.3 MiB/s M->W direct; check if this is tailnet (DERP/relay) or a core send-path limit.

## Manual steps for user (Windows app / UI, not run)
- Build and launch the Windows app on the desktop (`apps/windows`), login with the test account (pairtest-1014@example.test, or a new one).
- Pair Windows app with Mac app: confirm SAS matches on both screens.
- Copy text on Mac, confirm it appears in Windows app; copy on Windows, confirm on Mac.
- Send a file from each app to the other, confirm hash.
- Remove device from one side, re-pair.

## Cleanup done
- Mac: backend, relay spike, SSH reverse tunnel killed. No fsync-cli processes left.
- Windows: `stop-all.ps1` run; no pairtest/fsync-cli processes. Worktree kept at 85f8968, clean.
- Docker: removed `fsync-pairtest-pg-1014` (with volumes). Mac integration script's own PG (`fsync-mac-it-pg-44150`) removed by its trap. `mystifying_euclid` and other containers untouched.
- Left as-is: scratch dirs under `pairtest/` and Windows `C:\Users\dev\pairtest-scratch\` (data and run outputs, not repo files).
- A `cargo build --release` (pid 39054) from another session was running during the test. Not mine, not touched.

## Unresolved questions
1. Why the first W->M clip took 92 s to reach the Mac, and why `clip watch --count 1` did not exit after receiving it.
2. Is the lazy text fetch origin-dependent after `clip put` exits? (Hypothesis, not tested.)
3. Does direct (non-relay) LAN path behave the same? Blocked by L2 reachability.
4. Windows app UI flows: not run.

**Status:** DONE_WITH_CONCERNS
**Summary:** Pairing (code + SAS), clip and file sync (64 MiB, hash-verified) pass both directions, direct and forced relay, over Tailscale. Remove/re-pair pass. Mac app integration passes 3/3 (local). One W->M direct clip arrived 92 s late and the watch exited with timeout; not reproduced in later runs. Windows UI not run (manual).
**Concerns/Blockers:** LAN unreachable (tests over tailnet, path label "Lan" misleading). Relay spike binds 127.0.0.1 only. Windows clock unsynced. Intermittent text-clip delay unresolved (owner: core). Full list of unresolved questions above.
