# E2E test results

Machine-written metrics from the e2e harness. One file per run: `netns-<YYMMDD-HHMM>.json`.

## Produce

```
tests/e2e/netns-matrix.sh                 # needs Docker; builds image fsync-netns:local, runs everything in a privileged container
tests/e2e/run-matrix.sh --scenarios netns # same, through the matrix runner
NETNS_MIB=16 NETNS_SCENARIOS=n1,n5 tests/e2e/netns-matrix.sh   # subset, smaller file
```

Env: `NETNS_MIB` (default 64), `NETNS_SCENARIOS` (csv ids), `QUARANTINE` (csv ids forced to QUARANTINED),
`SKIP_BUILD=1`, `CLEAN_CACHE=1`, `CLIP_MAX_S` (default 2), `SCN_TIMEOUT_S` (default 300), `KEEP=1`.
All Docker objects are named `fsync-netns-*` and labelled `fsync.e2e=netns`; the wrapper removes them on exit.

## Result values

| result | meaning |
| --- | --- |
| PASS | all assertions held |
| FAIL | an assertion failed; fails the run only when `gating` is true |
| SKIP | not runnable here; `notes` carries the reason |
| QUARANTINED | known product or environment gap, evidence in `notes`; never counted as a pass |

## Schema

Top level: `run_id`, `git_sha`, `mib`, `docker{cpus,mem_gb}`, `duration_s` (matrix wall, excludes build),
`build_s`, `scenarios[]`.

Scenario: `id`, `result`, `gating`, `secs`, `path_expected`, `path_observed` (`lan|direct|relay`, from
interface byte counters), `bytes{relay,wan_a,wan_b,host_a}`, `clip_latency_s`, `file_secs`, `mib_s`,
`sha_ok`, `cli_path` (what the CLI reports; `Lan` is printed for any IP path), `notes`.
