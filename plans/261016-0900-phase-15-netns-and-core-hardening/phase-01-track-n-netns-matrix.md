# Phase 01 — Track N: netns network matrix

## Context Links
- Spec: [MVP phase 15](../261013-2029-fsync-native-mvp/phase-15-integration-testing-cross-os.md) (N1-N9, security cases)
- Harness: `tests/e2e/run-matrix.sh`, `tests/e2e/p0-exit-demo.sh` (login/OTP/pair/clip/file rounds), `tests/e2e/compose-stack-sync.sh`
- Path tiers: `core/fsync-core/src/net/lan_first_path_selector.rs` (Lan = remote IP on an on-link subnet, Direct = other IP, Relay)
- Four-track [plan](../261015-0410-four-track-hardening/plan.md) Next action 12

## Overview
- Priority P1 | Status: Completed (N3, N4 un-quarantined 2026-10-17 via Track N3; strict netns 10 PASS on c429406) | Effort 2.5d | Branch `track-n/netns-matrix` (worktree off main 68d7ae6), merged 05890e3
- One privileged Linux container on this Mac builds fsync-cli, backend and the spike relay for aarch64, wires netns topologies, runs N1-N9, asserts path from veth byte counters, writes metrics JSON.
- Ownership: `tests/e2e/**`, `docs/test-results/**` only. Product bugs found are filed as findings (report), never fixed here.

## Key Insights
- p0 already proves the CLI flow (login via dev OTP in backend log, pair offer/join with SAS, clip put/watch, send/fetch, `via` field). Reuse it, do not reinvent.
- CLI `via` today is `Lan` for ANY IP path (hole-punched direct too): useless for N3. Byte counters are the truth.
- Client TLS uses webpki roots only, so a local relay has no trusted QUIC address discovery (QAD). Hole punching in N3 may then fail. Spike first (step 2), outcome recorded either way.
- `ensure_account` runs on every CLI start; N2/N9 reveal whether it survives an unreachable backend. Failure = finding, not harness bug.
- Pair once, reuse data dirs: an identity can run in any netns (`ip netns exec`). Saves ~30 s per scenario. N8 (destroys pairing) runs last.
- Host is macOS bash 3.2; container is bash 5. Anything sourced by p0 must stay bash-3.2 safe.

## Requirements
Functional
- `tests/e2e/run-matrix.sh --scenarios netns` runs the matrix (relay axis n/a, like `stack`). Env `NETNS_SCENARIOS=n1,n2,...` (default all), `MIB` (default 64), `KEEP=1`.
- Per scenario: clip 1 KiB text, file `MIB` MiB, sha256 compare, path classified from counters, metrics row.
- Priority order: N1, N2, N5 -> N3, N4 -> N7, N8, N9 -> N6 (best-effort) -> security (best-effort).
Non-functional
- Warm run (build cache hot) < 15 min wall; cold build reported separately, not counted.
- Deterministic: no internet from netns, fixed subnets, no sleeps as assertions (poll with deadline).
- Container memory under Docker's 8 GB: `CARGO_BUILD_JOBS=6`.

## Architecture
```
Mac host: run-matrix.sh -> netns-matrix.sh
  docker network fsync-netns-$RUN  (label fsync.e2e=netns)
   ├─ fsync-netns-pg-$RUN   postgres:17
   └─ fsync-netns-run-$RUN  --privileged, image fsync-netns:local (rust:1.99-bookworm + iproute2 iptables python3 curl)
        mounts: worktree -> /src:ro, vol fsync-netns-cargo -> cargo registry, vol fsync-netns-build -> /build, $WORK -> /out
        netns/run-in-container.sh:
          root ns: backend (dev mode) on 100.64.0.1:8080, br-wan 100.64.0.0/24, DATABASE_URL -> pg via docker DNS
          ns relay  100.64.0.2  (spike relay; its veth = relay byte counter)
          ns r-lan  wan 100.64.0.10, lan br 192.168.10.0/24 -> ns h-la .10, h-lb .11       (N1 N2 N5 N6 N7 N8 N9)
          ns r-a    wan 100.64.0.21, lan 10.1.0.0/24 -> ns h-na .10                         (N3 N4)
          ns r-b    wan 100.64.0.22, lan 10.2.0.0/24 -> ns h-nb .10                         (N3 N4)
        root ns does NOT forward netns traffic to eth0 (no internet).
```
NAT modes on r-a/r-b (flush + reapply per scenario):
- full-cone: `SNAT --to wanIP` + `PREROUTING -i wan DNAT --to host` (1:1, endpoint-independent mapping and filtering).
- symmetric: `MASQUERADE --random-fully` + FORWARD drop of unsolicited inbound.

Path classification (lib/assert.sh `classify_path SIZE`), deltas over the file round only:
- `relay` if relay veth delta >= 0.8*SIZE
- `lan` if relay delta < 0.1*SIZE and A's router WAN veth delta < 0.1*SIZE and A's host veth delta >= SIZE
- `direct` if relay delta < 0.1*SIZE and router WAN delta >= 0.8*SIZE
- else `mixed` (fail, counters dumped)
Counters: `/sys/class/net/<veth>/statistics/{rx,tx}_bytes` read via `ip netns exec`.

Scenario expectations (gating unless marked):
| # | Setup | Expect |
|---|---|---|
| N1 | h-la/h-lb, WAN up | path lan, hash ok, clip latency recorded (gate < CLIP_MAX_S=2; spec target 0.3 recorded) |
| N2 | r-lan WAN link down after pairing | path lan via mDNS, hash ok |
| N5 | N1 + `--force-relay` | path relay, MiB/s logged |
| N3 | h-na/h-nb, full-cone both | path direct (see QAD risk) |
| N4 | symmetric both | path relay, clip < 1.5 s recorded (gate CLIP_MAX_S) |
| N7 | B stopped | A `devices --json` shows B offline within 60 s; clip put while offline not delivered after B returns (watch 10 s gets 0); new clip after return delivered |
| N9 | backend process stopped, relay up | LAN clip + file still work (cached list) |
| N8 | A `account reset-trust --yes` (fresh login code first) | B's clip and send to A rejected within 30 s; A receives nothing |
| N6 best-effort | mid-transfer: re-address h-la (ip addr del/add) or swap route | fetch completes, hash ok; non-gating |
| Sec best-effort | N8 doubles as "unpaired NodeId dial" (B dials A's gate after removal). Pure unpaired dial from a fresh account is not expressible (CLI resolves peers locally): SKIP with reason; covered by core tests `clip_security.rs`, `net_gate_and_paths.rs` |

Metrics JSON `docs/test-results/netns-<YYMMDD-HHMM>.json`:
`{run_id, git_sha, mib, docker:{cpus,mem_gb}, duration_s, scenarios:[{id, result: PASS|FAIL|SKIP|QUARANTINED, gating, path_expected, path_observed, bytes:{relay,wan_a,wan_b,host_a}, clip_latency_s, file_secs, mib_s, sha_ok, cli_path, notes}]}`. `cli_path` = CLI `path` field if present (Track K), else null; never asserted.

## Related Code Files
Create
- `tests/e2e/netns-matrix.sh` — host entry: image build, network, pg, runner, copy JSON, cleanup by name/label
- `tests/e2e/netns/Dockerfile` — runner image
- `tests/e2e/netns/run-in-container.sh` — build, infra start, pair once, scenario loop, metrics
- `tests/e2e/lib/netns-topology.sh` — ns/veth/bridge create+destroy, `nat_mode`, `wan_down/up`, `counters`
- `tests/e2e/lib/assert.sh` — `fail`, `assert_eq`, `assert_lt`, `sha_eq`, `classify_path`, `metric_add`, `metrics_write`
- `tests/e2e/lib/cli-session.sh` — `wait_for`, `jfield`, `otp_for`, `login`, `pair`, `clip_round`, `file_round` (bash-3.2 safe)
- `tests/e2e/scenarios/n1-lan.sh` … `n9-backend-down.sh` (one function each, sourced)
- `docs/test-results/README.md` (schema, how to run) + first green `netns-*.json`
Modify
- `tests/e2e/run-matrix.sh` — add `netns` to `script_of`, `--list`, header; run once like `stack`
- `tests/e2e/p0-exit-demo.sh` — source `lib/cli-session.sh` instead of inline helpers (DRY). Only if p0 regression PASSes; else revert this one change and keep duplication noted.
Delete: none.

## Implementation Steps
1. Worktree: `git worktree add ../fsync-native-track-n -b track-n/netns-matrix 68d7ae6`.
2. Spike (0.5d, timeboxed): runner image + build in container (cache volumes) + one LAN topology + p0-style pair/clip/file. Check: mDNS across a netns bridge, iptables backend (nft vs legacy) on Docker Desktop kernel, `--random-fully` support, N3 hole punch with spike relay (QAD). Record findings in report.
3. `lib/netns-topology.sh` + `lib/assert.sh` + `lib/cli-session.sh`; idempotent teardown (`ip -all netns delete` inside container only).
4. `netns-matrix.sh` host wrapper: labels, names `fsync-netns-*`, EXIT trap removes only its own container/network/pg; never volumes unless `CLEAN_CACHE=1`.
5. Scenarios N1, N2, N5; get them green. Wire `run-matrix.sh netns`.
6. N3, N4 (NAT modes). If N3 cannot go direct due to QAD: mark QUARANTINED with finding, escalate Unresolved Q1 to lead.
7. N7, N9, then N8 (last; consumes pairing).
8. N6 and security best-effort.
9. Metrics JSON + README; p0 refactor + p0 regression (MIB=16).
10. Full warm run, time it, commit JSON. Report to `plans/reports/` (path from hook naming).

## Todo List
- [x] Worktree + spike (mDNS, iptables, QAD/N3 feasibility)
- [x] lib/netns-topology.sh, lib/assert.sh, lib/cli-session.sh
- [x] netns-matrix.sh + Dockerfile + run-in-container.sh
- [x] N1, N2, N5 green (N2 needed K backend-down start)
- [x] N3, N4 un-quarantined 2026-10-17 (Track N3: test relay CA + QAD hook; N4 root cause was iroh net report waiting ~3 s for QAD, not the dialer). N3 direct clip 0.01 s; N4 relay clip 0.01 s. Owner Q1/Q2 answered by this track
- [x] N7, N9, N8
- [x] N6 best-effort, security best-effort (sec SKIP, reason recorded)
- [x] run-matrix.sh `netns` scenario
- [ ] metrics JSON committed (JSON exists, untracked; docs/test-results/README.md exists)
- [x] p0 refactor + regression PASS (MIB=16)
- [x] warm run < 15 min (~3 min at MIB=64); fmt/clippy/test workspace unaffected

## Success Criteria
- `tests/e2e/run-matrix.sh --scenarios netns` exits 0; every gating scenario PASS or QUARANTINED with a lead-approved finding.
- N1 observed `lan`, N5 and N4 `relay`, N3 `direct` (or quarantined per Q1); all hashes match.
- Warm wall time < 15 min printed by the wrapper; JSON validates (python `json.load`) and is committed.
- After the run: `docker ps -a --filter label=fsync.e2e=netns` empty; `mystifying_euclid` state unchanged.
- Negative check: forcing a wrong expectation (e.g. N1 expect relay) makes the run FAIL (proves assertions bite).

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| No QAD on local relay -> N3 never direct | H x M | Spike step 2; quarantine + finding; Q1 to lead |
| Docker Desktop kernel lacks iptables feature (nft/random-fully) | M x H | Spike; fall back to `iptables-legacy`; symmetric via SNAT `--to wanIP:port-range --random` |
| mDNS multicast not crossing netns bridge | M x M | `mcast_snooping 0` on bridge; else N2 finding |
| Cold build slow / OOM at 8 GB | M x M | persistent volumes, `CARGO_BUILD_JOBS=6`, build excluded from 15 min |
| `ensure_account` needs backend -> N2/N9 fail | M x M | finding to K owner/lead (Q2), quarantine |
| Presence-dependent timing flakes (N7) | M x M | poll with deadlines, record values, retry only on infra errors |
| Touching unrelated containers | L x H | name/label-scoped cleanup only; reviewer checks every `docker` call |
| p0 refactor regresses p0 | L x M | p0 MIB=16 regression gate; revert that change if red |

## Rollback
Additive; revert merge commit. `docker volume rm fsync-netns-cargo fsync-netns-build` to free disk.

## Security Considerations
Dev-mode backend with throwaway Postgres and `@example.test` emails; no secrets, no hosted relays, no VPS. Privileged container is local-only, no published ports.

## Next Steps
Lead: tick MVP phase-15 todos, link JSON. Later: nightly Linux CI job (needs GitHub billing), lab runners, 1 GiB variant.

## Results (merged main 026fb5a)
- Reviews: 8.8, then 9.5 (0 critical). Final run at MIB=64: n1, n2, n5, n3b, n6, n7, n8, n9 PASS. n3, n4 QUARANTINED. sec SKIP.
- Strict mode (`NETNS_STRICT=1`) red only from n3/n4. Non-strict rc 0.
- Metrics: `docs/test-results/netns-261016-1643.json` (strict), `netns-261016-1649.json`.
- Negative check: N1 expect relay FAILs. p0 MIB=16 PASS. Docker: no fsync-netns leftovers.
- Quarantine causes: N3 no QUIC address discovery on local relay. N4 relay path OK, first connect ~4 s (iroh net report ~3 s, see Track K round 2 B).
- JSON committed in 366c70d.

## Results (2026-10-17, local main, not pushed)
- Track N3 merged (a2c9a2d; branch commits fa3e2e3, d1c35f3, e955881, 390bd94). Review 9/10, then 9.5 after guard fix.
  - Feature-gated `test-relay-ca` hook in fsync-core (env `FSYNC_TEST_RELAY_CA`, `NodeBuilder::extra_relay_ca_pem`). Test-only, no prod effect.
  - Relay TLS + QAD enabled in the netns harness. Private-CA TLS handshake test `relay_private_ca.rs`.
  - Build-script guards (xcframework + Windows DLL) fail if `test-relay-ca` or `test-hooks` reach fsync-ffi (`cargo tree --format '{p} {f}'`; positive and negative control verified).
- Results: N3 direct, clip 0.01 s. N4 relay, clip 0.01 s (was ~4 s).
- Strict netns on d1c35f3 (pre-merge): 10 PASS, 1 SKIP (sec).
- netns strict on c429406 (tree dirty with plan markdown only): PASS=10 FAIL=0 QUARANTINED=0 SKIP=1 (sec), rc 0, 259 s, `docs/test-results/netns-261017-1821.json`.
- Prod relays run `RELAY_QAD=0`. QAD enable is a separate owner decision (see plan.md).
