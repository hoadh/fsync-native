# E2E test results

Machine-written metrics from the e2e harness. One file per run: `netns-<YYMMDD-HHMM>.json`.

## Produce

```
tests/e2e/netns-matrix.sh                 # needs Docker; builds image fsync-netns:local, runs everything in a privileged container
tests/e2e/run-matrix.sh --scenarios netns # same, through the matrix runner
NETNS_MIB=16 NETNS_SCENARIOS=n1,n5 tests/e2e/netns-matrix.sh   # subset, smaller file
NETNS_STRICT=1 tests/e2e/netns-matrix.sh  # a QUARANTINED gating scenario fails the run
```

Env: `NETNS_MIB` (default 64), `NETNS_SCENARIOS` (csv ids), `NETNS_STRICT=1`, `QUARANTINE` (csv ids forced to
QUARANTINED), `SKIP_BUILD=1`, `CLEAN_CACHE=1`, `CLIP_MAX_S` (default 2), `SCN_TIMEOUT_S` (per scenario, default 300),
`NETNS_MAX_S` (host-side bound for the whole run, default 2400; removes this run's containers and network by exact
name), `KEEP=1`.

All Docker objects are named `fsync-netns-*` and labelled `fsync.e2e=netns`; the wrapper removes them on exit.
The cache volumes `fsync-netns-cargo`, `fsync-netns-rustup` and `fsync-netns-build` have fixed names, so
concurrent runs are not supported.

## N8 (trust reset) and the refusal ack

N8 asserts that after A resets trust, every B `clip put` and the B file push fail with `not paired with that device`
(the typed refusal) and FAILs if any is reported delivered. Run it alone, and its core counterpart:

```
NETNS_MIB=16 NETNS_SCENARIOS=n8 NETNS_STRICT=1 tests/e2e/netns-matrix.sh
cargo test -p fsync-core --test clip_refusal_feedback
```

## Result values

| result | meaning |
| --- | --- |
| PASS | all assertions held |
| FAIL | an assertion failed; fails the run only when `gating` is true |
| SKIP | not runnable here; `notes` carries the reason |
| QUARANTINED | known product or environment gap, evidence in `notes`; never counted as a pass |

The run prints `summary: PASS=.. FAIL=.. QUARANTINED=.. SKIP=..` and, when a gating scenario is quarantined, a
`WARNING` line naming it. By default that does not fail the run; with `NETNS_STRICT=1` it does.
A quarantined scenario that starts working promotes itself to PASS (N2, N9 once the CLI starts without the
backend; N3 once a direct path appears).

## Schema

Top level: `run_id`, `git_sha` (HEAD of the tested tree, suffixed `-dirty` when files other than past results
differ), `mib`, `strict`, `docker{cpus,mem_gb}`, `duration_s` (matrix wall, excludes build), `build_s`,
`counts{PASS,FAIL,QUARANTINED,SKIP}`, `quarantined_gating[]`, `scenarios[]`.

Scenario: `id`, `result`, `gating`, `secs`, `path_expected`, `path_observed` (`lan|direct|relay|mixed`, from
interface byte counters), `bytes{relay,wan_a,wan_b,host_a}`, `clip_latency_s`, `file_secs`, `mib_s`, `sha_ok`,
`cli_path`, `cli_path_tier`, `notes`.

`cli_path` / `cli_path_tier` are what the CLI itself reports (`path` on `sent`, `path_tier` on `fetched`). They are
informational and null when the CLI does not emit the field; the byte-counter classification is authoritative.

## Path classification

Computed from byte-counter deltas over the file transfer only (SIZE is the file size):

| path | rule |
| --- | --- |
| relay | relay-namespace WAN bytes >= 80% of SIZE |
| lan | relay < 10% of SIZE, A's router WAN < 10% of SIZE, A's host eth0 >= SIZE |
| direct | relay < 10% of SIZE, A's router WAN >= 80% of SIZE |
| mixed | anything else; fails every expectation |

Control traffic is about 30 KB, so the thresholds are meaningful from 1 MiB up (the default sizes are 16 and 64 MiB).
