# P0 Rust Track Final Test Run

**Status:** DONE_WITH_CONCERNS. Final pass (post round-7 pairing change) all green: fmt, clippy, workspace tests (236/0 failed), xcframework, swift build/test, e2e. Concern: leftover e2e temp dirs, and gate 5 not re-run on final tree.

**Env:** macOS arm64, 32 GB. Rust 1.99.0. Default `CARGO_TARGET_DIR` (`CARGO_TARGET_DIR=.../target/test` blocked by scout-block hook on the word "target"). Tree frozen. No source edits by tester.

## Pass 2 (final, post round-7): verdict basis

| Gate | Result | Time |
|---|---|---|
| fmt --all --check | PASS | <1s |
| clippy --workspace --all-targets -D warnings | PASS, 0 warnings | 3.5s incremental |
| test --workspace --no-fail-fast | PASS, 236 passed, 0 failed, 1 ignored | 134s |
| build-xcframework.sh | PASS | 80s |
| swift build (FsyncCoreKit) | PASS | - |
| swift test (FsyncCoreKit) | PASS, 6 tests, 0 failures | 3s |
| e2e p0-exit-demo.sh (once) | PASS | 104s |
| cleanup check | clean | - |

New test file present: `tests/pairing_repair_after_reset.rs` (2 passed). Accounts for the +2 vs pass 1 (234 to 236).

## Pass 1 (pre round-7): superseded, kept for history

| Gate | Result | Time |
|---|---|---|
| fmt | PASS | <1s |
| clippy -D warnings | PASS | 3s (incremental) |
| test --workspace --no-fail-fast | PASS, 234 passed, 0 failed, 1 ignored | 160s |
| build-xcframework.sh | PASS | 307s (cold) |
| swift build / swift test | PASS / PASS, 6 tests, 0 failures | 2s / 3s |
| spikes/iroh-discovery cargo build | PASS | 51s |
| e2e x2 | PASS / PASS | 229s / 38s |

## Gate 3 per-package counts (pass 2)

Per-binary counts from log. Package attribution is by test binary name, so approximate outside the backend.

| Package | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|
| fsync-backend (lib 19 + 13 integration binaries) | 71 | 0 | 0 | integration: account_delete 2, auth_otp 3, auth_otp_limits 9, auth_refresh 5, devices_chain 4, devices_limits 5, devices_reset 4, header_read_timeout 2, pairing 4, presence_auth 2, presence_limits 7, presence_updates 2, session_revocation 3 |
| fsync-core (unit 11 + integration) | see total | 0 | 0 | e.g. blob_contract 7, clip_protocol 5, encrypted_blobs 6, file_transfer 7, history_db 10, pairing_iroh 7, pairing_hardening 6, trust_manager 7 |
| fsync-cli | see total | 0 | 0 | cli_surface 6 |
| fsync-ffi, fsync-proto | 0 | 0 | 0 | doc-tests only, no unit tests |
| Remainder (core/cli/ffi/proto, incl. new pairing_repair_after_reset 2) | 165 | 0 | 1 | ignored: `file_ranges::bench_1_gib_loopback_and_clip_latency` (benchmark, by design) |

Total: 236 passed, 0 failed, 1 ignored. Pass 1 per-binary counts matched except the 2 new tests.

Finer per-crate split for core vs cli vs ffi was not separated in the log. Re-run with `-p` per crate if needed.

## E2E metrics (p0-exit-demo.sh)

| Round | Clip direct A to B | Clip relay-only A to B | Settle direct / relay | File 256 MiB direct (via=Lan) send / recv | File 256 MiB relay (via=Relay) send / recv |
|---|---|---|---|---|---|
| Pass 1, round 1 | 0.02 s | 0.03 s | 414 / 307 ms | 47.6 / 32.9 MiB/s | 99.6 / 53.0 MiB/s |
| Pass 1, round 2 | 0.06 s | 0.02 s | 410 / 208 ms | 50.3 / 36.4 MiB/s | 104.8 / 57.7 MiB/s |
| Pass 2 (final), round 1 | 0.04 s | 0.06 s | 418 / 311 ms | 55.3 / 38.2 MiB/s | 105.4 / 58.9 MiB/s |

Clip latency under the 5 s bound on all rounds. All hashes verified (script PASS). Pass 2 ran one round, per instructions. The script runs direct and relay-only rounds internally, so each line above is one script run.

## Gate 7 cleanup

- No fsync-cli, fsync-backend, iroh-relay, or p0-exit-demo processes.
- No fsync-e2e containers.
- Concern: six `tmp.*` dirs under `/private/var/folders/x8/.../T/` remain. Some hold `backend.log` and `login-A/B.json`, so likely from earlier interrupted e2e runs (not mine, not deleted). Clean them up or confirm they are safe to remove.

## Concerns

1. Gate 5 (spikes/iroh-discovery build) passed in pass 1 only. Not re-run on the post-round-7 tree. Pairing change is in core, not the spike, so low risk, but not verified.
2. Pass 1 gate 3 failure (`devices_reset` `PortNotExposed`) is resolved per coordinator (pg.rs port-lookup retry). Pass 2 full run was green, but a single green run does not prove flake-free. The coordinator reports 3 consecutive green runs of `devices_reset`.
3. xcframework rebuild needed for swift tests to reflect the round-7 change. Done in pass 2 (80s).
4. Leftover e2e temp dirs (see gate 7).

## Unresolved questions

- Are the six leftover `tmp.*` dirs from prior runs of this session or older? Safe to delete?
- Should gate 5 be re-run on the final tree for completeness?
