# Track N: netns matrix (phase 15). Report saved by the lead from the agent's output

- Worktree: `.claude/worktrees/agent-a8d87e328e3fc3dd0`, branch `track-n/netns-matrix` (base 68d7ae6).
- Commits:
  - 37ec6c6 refactor(e2e): shared assert/cli-session helpers
  - 13245d1 feat(e2e): netns matrix in privileged container
  - 189a639 test(e2e): results + schema
- Results: `docs/test-results/netns-261016-1411.json`. Run: `NETNS_MIB=64 tests/e2e/run-matrix.sh --scenarios netns`, rc 0, 182 s matrix wall time. A cold build adds about 3 min once.

| id | result | gating | secs | notes |
|---|---|---|---|---|
| n1 LAN | PASS | y | 4 | path lan, clip ~0.01 s, 100+ MiB/s |
| n2 WAN down | QUARANTINED | y | 2 | CLI cannot start without the backend |
| n5 force-relay | PASS | y | 3 | relay bytes ~2x the file |
| n3 full-cone NAT | QUARANTINED | y | 16 | stays on relay; the local relay has no QUIC address discovery |
| n3b stateful firewalls (extra) | PASS | y | 16 | direct path |
| n4 symmetric NAT | QUARANTINED | y | 17 | relay, hash ok; clip 4.0 s vs the 1.5 s spec |
| n7 offline peer | PASS | y | 29 | offline after ~3 s; stale clip not delivered |
| n9 backend down | QUARANTINED | y | 0 | same error as n2 |
| n6 re-address, 1 GiB | PASS | n | 43 | survives the re-address; hash ok |
| n8 reset-trust | PASS | y | 45 | intermittently quarantines (finding 4) |
| sec unpaired dial | SKIP | n | 0 | not expressible via the CLI; covered by core tests |

Gates:
- p0 at MIB=16: PASS after the refactor.
- `shellcheck -x -S warning`: clean.
- A negative check (`N1_EXPECT_PATH=relay`) fails as expected.
- No leftover Docker objects.

## Findings
1. With the backend unreachable, the CLI exits at startup with `error: network: error sending request`. This blocks N2 and N9. Core fix needed.
2. The local relay has no QAD and the client trusts webpki roots only, so N3 cannot hole-punch. A core test hook is needed (extra relay CA + QAD).
3. The first connect for routed or NAT'd peers takes about 4 s. Suspected cause: the 2 s FIRST_ATTEMPT retry in `net/dialer.rs` (untraced). This blocks N4's 1.5 s target.
4. N8: after A resets trust, B's `clip put` sometimes exits 0 with `delivered:[A]`. Security holds, because A stores nothing.
5. The CLI's `via` reports `Lan` for any IP path. The harness uses byte counters instead.
6. Two CLIs on one data dir collide on `engine.lock` (expected).

## Unresolved questions
- Should a relay CA + QAD test hook be added for N3?
- Who fixes CLI startup without the backend (N2/N9)?
- Is the 4 s first-connect stall expected?
- Should the sender learn about the refusal in N8?
- Keep N3b as a permanent scenario?

## Round 2 (review fixes; commits 0140eae, d8331a5, 4920606)
- Full matrix at MIB=64: rc 0, 176 s. JSON `docs/test-results/netns-261016-1429.json`, git_sha 0140eae (clean). Summary `PASS=6 FAIL=0 QUARANTINED=4 SKIP=1`; quarantined gating: n2 n3 n4 n9.
- Fixed:
  - `NETNS_STRICT=1` fails on any gating quarantine (verified on n2).
  - Counts are in the JSON.
  - `cli_path`/`cli_path_tier` are filled when the CLI emits them.
  - Host watchdog `NETNS_MAX_S` removes leftovers by exact name.
  - Addresses are reset between scenarios.
  - Refusal error text is asserted.
  - The history query fails on a command error.
  - README documents the thresholds and that concurrent runs are unsupported.
- N8 root cause is in core: `publish()` reports `delivered` once the transport acks the frame. The gate closes an untrusted conn with 403 only after the handshake, so a removed sender is told `delivered`. Rate: 2/3 in the matrix, 3/3 alone. A-side security checks still gate; the finding is recorded in notes on every run.
- p0 at MIB=16: PASS. shellcheck clean. No Docker leftovers.
- Open:
  - Rerun with `NETNS_STRICT=1` after the Track K merge.
  - Fix N8 sender feedback in core (close before ack, or app-level accept ack)?
  - Keep n3b?
