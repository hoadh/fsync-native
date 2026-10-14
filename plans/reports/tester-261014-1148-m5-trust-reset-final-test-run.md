# M5 trust-reset API: final test gates

Date: 2026-10-14. Tree: uncommitted M5 change (42 entries in `git status`). Host: macOS arm64.
No product code edited. Default cargo target dir used.

## Gate results

| # | Gate | Result |
|---|------|--------|
| 1 | `cargo fmt --all --check` | PASS (exit 0) |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS (exit 0, no warnings) |
| 3 | `cargo test --workspace --no-fail-fast` | PASS: 253 passed, 0 failed, 1 ignored (62 test binaries). Rerun not needed. |
| 4a | `scripts/build-xcframework.sh` | PASS (exit 0). Universal lib x86_64+arm64, xcframework written. |
| 4b | `swift test` (FsyncCoreKit) | PASS: 8 XCTest, 0 failures. 0 swift-testing tests. |
| 5 | `tests/e2e/p0-exit-demo.sh` | PASS (exit 0), 1m46s total |
| 6 | Cleanup | Clean (see below) |

### Gate 3 detail
- Ignored: `file_ranges::bench_1_gib_loopback_and_clip_latency` (1 GiB benchmark, `#[ignore]` by design).
- Slowest binaries: `file_transfer` 20.3s, `encrypted_blobs` 15.3s, `file_stream_cap` 10.1s.
- Pairing (mDNS) binaries `pairing`, `pairing_iroh`, `pairing_repair_after_reset`: all pass. No flake observed.
- Wall time 2m50s.

### Gate 4 detail
- Script rebuilt release `fsync-ffi` (cold compile 1m07s, warm rebuild ~12s). Output is gitignored (`build/`, `FsyncFFI.xcframework/`).
- Script regenerated tracked file `apps/macos/Packages/FsyncCoreKit/Sources/FsyncCoreKit/fsync_ffi.swift`. It was already modified before this run. I did not diff against a pre-run copy, so I cannot confirm the regeneration was byte-identical to the uncommitted version. See questions.
- No compiler warnings in the script log.

### Gate 5 detail (p0 exit demo)
Flow: login A (created) and B (needs_pairing), pairing with SAS match (`fiko niri fito`), then clip and 256 MiB file each in direct and relay-only rounds. Sha256 and cmp verified on both files.

| Round | Clip latency | Presence settle | File send | File recv | Via |
|-------|--------------|-----------------|-----------|-----------|-----|
| direct | 0.03 s (limit 5 s) | 310 ms | 4.12 s, 62.1 MiB/s | 6.12 s, 41.8 MiB/s | Lan |
| relay-only | 0.04 s | 316 ms | 2.44 s, 104.8 MiB/s | 4.44 s, 57.6 MiB/s | Relay |

Devices list at end: devA and devB both `online:false` after processes exited. Expected, not a failure.

### Gate 6 detail (cleanup)
- Before run: no fsync/backend/relay/iroh processes; 5 pre-existing `tmp.*` dirs in `$TMPDIR`.
- After run: no fsync/backend/relay/iroh processes. No new `tmp.*` dirs (e2e `WORK` removed on PASS). No `fsync-e2e-*` containers.
- No testcontainers-labelled containers left (`org.testcontainers` label count = 0).
- Untouched: `mystifying_euclid` (postgres:17, Up) and the other pre-existing containers (mongodb, redis, rabbitmq, loki, plus Exited ones).

## Critical issues
None. All gates pass on first run. No failing tests to diagnose.

## Observations (non-blocking)
- Direct LAN file transfer was slower than relay-only in this single run (41.8 vs 57.6 MiB/s recv). The direct round ran first and may include warm-up cost. Single sample, so not conclusive.
- The xcframework script rewrites a tracked file (`fsync_ffi.swift`). Re-running it can silently change the diff. Consider committing the regenerated bindings or excluding them from the gate.

## Recommendations
1. Add a regen-diff check (`git diff --exit-code` after `build-xcframework.sh`) so generated-binding drift fails CI.
2. Repeat the e2e direct round 3x to get a stable LAN vs relay throughput baseline.
3. Consider running the 1 GiB benchmark (`--ignored`) once before release. It is skipped by default, so no current gate covers it.

## Next steps
1. Confirm `fsync_ffi.swift` regeneration matches the intended uncommitted content, then commit.
2. Optional: repeat e2e for throughput stability.

## Unresolved questions
- Was `fsync_ffi.swift` byte-identical after regeneration? Not verified (no pre-run copy).
- Is the direct-vs-relay throughput gap real or warm-up noise? Needs repeat runs.
- The 1 GiB benchmark is ignored by default. Should it be a release gate?

**Status:** DONE
**Summary:** All 6 gates pass on the uncommitted M5 change: fmt, clippy (-D warnings), 253 cargo tests (0 failed, 1 ignored by design), xcframework build, 8/8 Swift tests, and the P0 e2e demo (clip 0.03 s, 256 MiB file sha256-verified on Lan and Relay). No leftover processes, temp dirs, or testcontainers. `mystifying_euclid` left running.
**Concerns/Blockers:** None blocking. The xcframework script rewrites a tracked bindings file, so the regeneration diff is unverified. Direct LAN throughput was below relay in one sample.
