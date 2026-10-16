# Tester report: merged main (65b69a4, Track K on top of Track N)

Scope: verify local main after Track K core hardening merge with Track N netns matrix. No source edits, no commits, no pushes.

## 1. Static and unit checks

| Check | Result |
|---|---|
| `cargo fmt --all --check` | PASS (exit 0) |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (exit 0, no warnings) |
| `cargo test --workspace` | PASS (exit 0): 76 suites, **350 passed, 0 failed, 3 ignored** |

Ignored (by design): `net::local_subnets::tests::print_interface_verdicts` (diagnostic), `bench_1_gib_loopback_and_clip_latency`, `bench_200_mib_serial_256k_reads`.
Test wall time about 5m11s (incl. compile).

Note: no `Cargo.toml` in `core/`. Workspace root is repo root (includes `backend`). Commands run from repo root.

## 2. Netns matrix, strict, 64 MiB

Command: `NETNS_STRICT=1 NETNS_MIB=64 tests/e2e/run-matrix.sh --scenarios netns`
Result JSON: `/Users/alex/Code/fsync-native/docs/test-results/netns-261016-1530.json` (kept, not committed)
Run exit: 1 (strict). Summary: PASS=6 FAIL=2 QUARANTINED=2 SKIP=1.

| id | result | notes |
|---|---|---|
| n1 | PASS | LAN 64 MiB sha ok |
| **n2** | **FAIL** | `send failed: error: devB did not come online` (140 s) |
| n5 | PASS | forced relay, sha ok |
| n3 | QUARANTINED | no direct path behind full-cone NAT (local relay, no QUIC addr discovery) |
| n3b | PASS | direct, sha ok; clip 4.03 s |
| n4 | QUARANTINED | relay, clip 4.02 s > 1.5 s spec |
| n7 | PASS | |
| **n9** | **FAIL** | same error as n2 (124 s) |
| n6 | PASS (non-gating) | 1 GiB re-address, sha ok |
| n8 | PASS | FINDING in notes: B clip put reported delivered after A reset-trust in 0/3 tries; refused with `peer is offline` |
| sec | SKIP | not expressible via CLI |

Expected N2 and N9 PASS. Actual FAIL. The CLI now starts with backend down (probe passes, no quarantine). The failure moved to the file send step.

Strict failed for n2/n9, not only n3/n4. Per the step-2 condition, the non-strict rerun was NOT run.

Log: `/var/folders/x8/d1lt6nyx39l2nxzvp3r6vt_m0000gn/T/tmp.7xIAtpRj9f/netns-local.log`

### Root cause (N2/N9)

- `tests/e2e/scenarios/n2-wan-down.sh` / `n9-backend-down.sh`: `clip_round` then `path_round` → `cli B send devB --wait 60`.
- `core/fsync-cli/src/cmd_files.rs:24` `send()` calls `c.wait_online(node, wait)` before offering.
- `core/fsync-cli/src/ctx.rs:138` `wait_online` only polls status. It never dials. Status = `path_state` of live connections (`core/fsync-core/src/net/path_state.rs`), or backend presence (`service/devices.rs:33`). Backend is down, so nothing marks B online.
- Result: B shows `offline` in A's device list for the whole 60 s (clip settle also hit its 60 s cap: `settle_ms=60029`). Then `wait_online` bails.
- The offer itself would work: `core/fsync-core/src/service/clips.rs:95-137` `send_files` calls `publish_clip(push: true)`, which dials. The clip to B delivered in 0.02 s from the same state.
- Gating in `send()` ahead of the offer, and the 60 s wait with no dial, is the blocker. The `PeerOffline` retry loop below it is never reached.

Not verified: whether this is a regression from 8f2c6e7/3de20ec or a latent gap. Before this merge, N2/N9 stopped at the probe, so the file path was never exercised offline.

## 3. p0 scenario, 16 MiB

Command: `MIB=16 tests/e2e/run-matrix.sh --scenarios p0` → **PASS** (40 s).
- direct: clip 0.04 s, file 16 MiB sha ok, via=Lan (34.6 MiB/s)
- relay-only: clip 0.02 s, file 16 MiB sha ok, via=Relay (57.7 MiB/s)
- log: `/var/folders/x8/d1lt6nyx39l2nxzvp3r6vt_m0000gn/T/tmp.YPpsb0A1ht/p0-local.log`

## 4. Docker hygiene

- After netns run: 0 `fsync-netns-*` containers, 0 networks. Cache volumes `fsync-netns-build|cargo|rustup` remain (documented fixed names, kept).
- After p0: 0 `fsync-e2e*` containers left.
- `mystifying_euclid` untouched (still Exited 0).

## Critical issues

1. **N2 and N9 FAIL** (gating). Offline file send is blocked by `wait_online` in `cmd_files::send` (no dial, 60 s wait). Clip works offline, file does not. Fix direction: attempt the offer (which dials) with the `PeerOffline` retry loop, instead of gating on passive status. Or dial before waiting. Needs a product decision and a code change (not done here).
2. **N8 FINDING** (PASS, but notes flag it): after reset-trust, B clip put reports delivered 0/3 and refuses with `peer is offline`. Same class of issue (passive status, no dial). Worth a follow-up.
3. N3 and N4 still quarantined (known gaps, unchanged).

## Recommendations

1. Fix `cmd_files::send` so the offer dials first, or skip `wait_online` when backend is unreachable. Re-run N2, N9, N8.
2. Add a CLI-level test for file send with no presence and no live connection (offline start, peer on LAN).
3. Keep the strict-mode run as a gate only after N2/N9 pass. Otherwise it stays red by design.
4. Add `docs/test-results` README note: git_sha showed `-dirty` because of untracked `plans/reports/*` and `.claude/`.

## Unresolved questions

- Is the N2/N9 send gating a regression from the Track K merge (8f2c6e7 / 3de20ec) or a pre-existing gap? Not bisected.
- Expected N2/N9 PASS per brief; they are FAIL. Is the brief's expectation wrong about the send step, or is the fix supposed to land in this merge?
- `tests/e2e/README*` does not exist. Used `docs/test-results/README.md` and the p0 header in `tests/e2e/p0-exit-demo.sh`.
- Non-strict rerun not done (condition "only n3/n4" not met). Say if you want it anyway.
- `git_sha` `65b69a4-dirty` flag cause (untracked files only?) not checked.

**Status:** DONE_WITH_CONCERNS
**Summary:** Build, fmt, clippy, and 350 unit/integration tests pass. p0 16 MiB passes. Strict netns 64 MiB fails: N2 and N9 FAIL (not the expected PASS) because offline file send gates on `wait_online`, which never dials. N3/N4 remain quarantined.
**Concerns/Blockers:** N2/N9 gating bug (`core/fsync-cli/src/cmd_files.rs:24`, `ctx.rs:138`) blocks the expected netns result. Non-strict rerun skipped by condition.
