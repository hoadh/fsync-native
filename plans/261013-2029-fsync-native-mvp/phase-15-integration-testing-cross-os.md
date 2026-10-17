# Phase 15 — Cross-OS integration + network test matrix

## Context Links
- Design §7 success metrics; [phase-08](phase-08-core-history-ffi-cli.md) CLI harness; [phase-10](phase-10-infra-relay-backend-deploy.md) staging; apps [12](phase-12-macos-clipboard-files-history.md), [14](phase-14-windows-clipboard-files-history.md)

## Overview
- Priority: P1 | Status: done (N3/N4 un-quarantined 2026-10-17; security SKIP; netns strict green on c429406 (10 PASS, 1 SKIP); cross-OS runners, QA open) | Effort: harness 2 wk (B, P1 w2-5) + execution 2-3 wk (all, P1 w10-13)
- Automated CLI-level network matrix (Linux netns + real mac/win runners) + scripted manual QA for UI/clipboard/paste targets. Gate for release.
- Round 2 (2026-10-15, D7): `tests/e2e/run-matrix.sh` (scenario `p0`, relays local + sg) PASS: local 239 s, sg 51 s (~0.38 MiB/s, clip 0.53 s), MIB=8, watchdog and signal forwarding verified. Only the p0 scenario exists; none of the todo items below are done. Manual QA rows: `plans/reports/qa-checklist-261015-macos-manual.md` (macOS only).
- Round 3 (2026-10-16, main 026fb5a, [four-track phase 15 plan](../261016-0900-phase-15-netns-and-core-hardening/plan.md)): netns at MIB=64: n1, n2, n5, n3b, n6, n7, n8, n9 PASS; n3, n4 QUARANTINED (no QUIC address discovery on local relay; relay clip 4.0 s vs 1.5 s); sec SKIP (core tests cover). Strict run red only from n3/n4. p0 MIB=16 PASS. JSON: `docs/test-results/netns-261016-1643.json` (not yet committed).
- Round 4 (2026-10-17, local main, not pushed, [phase-15 plan](../261016-0900-phase-15-netns-and-core-hardening/plan.md)): Track N3 (test relay CA + QAD hook, build guards) un-quarantines N3 (direct, clip 0.01 s) and N4 (relay, clip 0.01 s). p0 MIB=16 PASS. Strict netns on c429406: 10 PASS, 0 FAIL, 1 SKIP (netns-261017-1821.json).
- Ad hoc runs 2026-10-14 (not the harness): Track X Mac↔Win over Tailscale (pair, clip, 64 MiB both ways, hash OK); public staging two-client e2e (direct + relay, 16 MiB hash OK); P0 e2e local relay. netns N1-N9, nightly, lab runners not started.

## Key Insights
- Network behavior is core-level → automate with `fsync-cli` (headless). Clipboard/paste-target behavior is UI-level → checklist on real machines (hard to automate Finder/Explorer).
- NAT simulation: Linux network namespaces + iptables (full-cone, symmetric) running CLI; real-world: home router + 4G hotspot.

## Requirements — Test matrix
| Layer | What | How | Where |
|---|---|---|---|
| Unit | DeviceList verify, frames codec, retention, mapper/filters, CF_HTML | cargo test / XCTest / xUnit | CI every PR |
| Integration (core) | 2-4 in-process endpoints: pair, clip, fetch, resume, tamper, unpaired reject | cargo test (local relay) | CI every PR |
| Integration (backend) | auth, list 409/422, pairing TTL, WS auth | testcontainers | CI every PR |
| Network E2E (CLI) | scenarios below | `tests/e2e/*.sh` netns | nightly Linux CI |
| Cross-OS E2E (CLI) | mac↔win LAN + relay | self-hosted runners / lab | nightly + pre-release |
| UI/manual | paste targets, filters, modes, history | QA checklist | pre-release |

Network scenarios (each: pair, clip text 1 KiB, image 5 MiB, file 1 GiB w/ mid-transfer kill+resume):
| # | Topology | Expect |
|---|---|---|
| N1 | same LAN, WAN up | direct LAN, ≥ 80% link, clip < 300 ms |
| N2 | same LAN, WAN down | LAN discovery works, direct |
| N3 | 2 NATs full-cone | direct after hole punch |
| N4 | symmetric NAT both | relay, clip < 1.5 s |
| N5 | `--force-relay` | relay, works, bandwidth logged |
| N6 | network switch mid-transfer (Wi-Fi→hotspot) | path migrates or resumes, hash ok |
| N7 | peer offline then online | status flips; no catch-up delivery (by design) |
| N8 | removed device | connections rejected immediately |
| N9 | backend down | existing paired peers on LAN still sync (cached list) |

Security cases: forged list, rollback list, unpaired NodeId dial, tampered chunk, path traversal rel_path, brute-force pairing code (rate limited).

Manual QA paste targets: Finder, Explorer, Desktop, Mail, Outlook, Teams, Slack, Chrome upload, VS Code; sensitive: 1Password, Bitwarden, KeePass, Keychain, Edge/Safari passwords.

## Architecture
```
tests/e2e/
  lib/netns-topology.sh, lib/assert.sh
  scenarios/n1-lan.sh … n9-backend-down.sh
  security/*.sh
  report/collect-metrics.sh   # latency/throughput/direct-ratio → JSON → docs/test-results/
docs/qa-checklist.md
```

## Related Code Files
- Create: above. Modify: none (bugs filed to owners of core/apps).

## Implementation Steps
1. netns topology lib + N1/N2/N5 first (P1 w2-3).
2. NAT scenarios N3/N4 + N6-N9; nightly workflow.
3. Mac + Windows lab machines / self-hosted runners for cross-OS CLI runs.
4. QA checklist; run per release candidate; record results.
5. Metrics collection → compare to targets.

## Todo List
- [x] netns lib + N1/N2/N5
- [x] N3/N4 (un-quarantined 2026-10-17: test relay CA + QAD hook); [x] N6-N9
- [ ] netns strict rerun on merged main (fill sha; JSON commit)
- [ ] Security cases (netns sec SKIP; core tests cover part)
- [ ] Cross-OS runners
- [ ] QA checklist + RC run
- [ ] Metrics report (JSON exists; commit pending)

## Success Criteria
- All N1-N9 + security cases green nightly 5 consecutive days; metrics meet design §7; QA checklist 100% must-items pass on Win10, Win11, macOS N and N-1.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Flaky network tests | H x M | deterministic netns, retries only on infra errors, quarantine list |
| Lab hardware availability | M x M | budget 2 Mac mini + 1 Win x64 + 1 Win ARM64 |

## Rollback
- Tests are additive; failing gate blocks release (phase 16), never bypassed.

## Security Considerations
- Test accounts only on staging; no real user data; secrets via CI env.

## Next Steps
- Green gate → phase 16 release.
