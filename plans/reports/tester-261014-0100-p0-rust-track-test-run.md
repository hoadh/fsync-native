# P0 Rust track test run (INTERIM)

Status: INTERIM. Stopped on coordinator request (another fix round starting). Results below are for frozen snapshots, NOT the final working tree. A fresh tester must run the final gates.

## Why snapshots
- Live tree in `/Users/alex/Code/fsync-native` was edited continuously during the run (files changed 00:18 to 00:35, again 00:49 to 00:59). Gates on the live tree were unreliable.
- Ran gates on frozen copies of source under the scratchpad: snap2 (00:36), snap3 = snap2 + 2 new test files (00:49), snap4 = live state at 00:59.
- Build-output dirs excluded from copies. No source files were modified in the live tree apart from the two new tests below.

## Gate results

| # | Gate | snap2 (00:36) | snap3 (+new tests) | snap4 (live 00:59) |
|---|---|---|---|---|
| 1 | `cargo fmt --all --check` | PASS | PASS | PASS |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS (80 s) | PASS (83 s) | PASS (incremental, all fsync crates checked) |
| 3 | `cargo test --workspace --no-fail-fast` | 219 pass / 0 fail / 1 ignored (191 s) | 226 pass / 0 fail / 1 ignored (224 s); rerun 226/0/1 (84 s) | 229 pass / 0 fail / 1 ignored (232 s) |
| 4 | xcframework + `swift build` + `swift test` | build OK (universal x86_64 arm64); swift build OK; XCTest 5/5 | not run | xcframework OK (345 s); swift build OK; XCTest 5/5 |
| 5 | `tests/e2e/p0-exit-demo.sh` | PASS (exit 0) | not run | FAIL (exit 1), see below |
| 6 | `cargo build` spikes/iroh-discovery | PASS (94 s) | not run | PASS (68 s) |

Ignored test: `core/fsync-core/tests/file_ranges.rs:114` "1 GiB loopback benchmark" (`#[ignore]` by design, not run).

### Per-crate counts (snap2, 219 total)
- fsync-backend: 19 unit + integration (account_delete 2, auth_otp 3, auth_otp_limits 9, auth_refresh 5, devices_chain 4, devices_limits 5, devices_reset 4, header_read_timeout 2, pairing 4, presence_auth 2, presence_limits 7, presence_updates 2, session_revocation 3). Testcontainers Postgres used.
- fsync-core: 8 unit + 11 integration-test targets (blob_contract 6, clip_protocol 5, clip_security 5, e2e_api 2, e2e_pair_flow 1, encrypted_blobs 6, file_ranges 3(+1 ignored), file_transfer 7, group_tree 2, history_db 7, history_engine 4, history_retention 6, history_settings_trust 2, net_gate_and_paths 5, pair_kdf 5, pairing_hardening 6, pairing_iroh 7, security_limits 10, service_facade 3, trust_head_pin_and_self_removal 2, trust_manager 7).
- fsync-proto: device_list_verify 5, names_and_caps 5, shape_caps 2, verify_next_chain 8, wire_api_presence 2, wire_clip_file 3, wire_frames 5.
- fsync-ffi: 0 tests before this run.
- fsync-cli: 0 tests before this run.
- snap3/snap4 add: presence_hello 2, sensitive_blob_refs 2, file_stream_cap (snap4), service_facade 4, plus new FFI/CLI tests below.

### Tests added by me (permitted paths, reported per instruction)
- `core/fsync-ffi/tests/error_mapping_roundtrip.rs`: 2 tests. Every service `FsyncError` variant round-trips through FFI `FsyncError` and back (service enum derives PartialEq). FFI `to_string()` equals service message for all 10 variants. Gap was real: FFI error mapping had no tests.
- `core/fsync-cli/tests/cli_surface.rs`: 5 tests (black-box via `CARGO_BIN_EXE_fsync-cli`, no network): help lists the 8 P0 subcommands; `--data` required (exit 2); unknown subcommand exit 2; `pair` needs subcommand exit 2; `clip put` needs `--text` exit 2. Gap was real: CLI had no tests. Another session reformatted this file after I wrote it (fmt only). Validated on snap3 only; snap4 includes it.

## Failures

### 1. fmt failure on mid-edit snapshot (snap1, 00:30), resolved on later snapshot
14 files with diffs (fsync-cli cmd_pair.rs, fsync-core api/*, blobs/export.rs, service/clips.rs, service/pairing.rs, tests/backend_harness). Not re-triaged. Clean on snap2/snap3/snap4.

### 2. clippy/compile failure on mid-edit snapshot (snap1), resolved on later snapshot
```
error[E0061]: this method takes 2 arguments but 1 argument was supplied
   --> core/fsync-core/tests/service_facade.rs:133:17
133 |     assert!(svc.pairing_confirm(true).is_err());
    |                 ^^^^^^^^^^^^^^^ ---- argument #1 of type `u64` is missing
error: could not compile `fsync-core` (test "service_facade") due to 1 previous error
```
`pairing_confirm(request_id, accept)` signature changed. Live tree later had `pairing_confirm(1, true)`. Clean on snap2+.

### 3. xcframework build failure on live tree at ~00:24 (transient)
```
error[E0432]: unresolved import `n0_watcher`   --> core/fsync-core/src/net/node_addrs.rs:5:5
error[E0599]: no method named `get` found for opaque type `impl Watcher<Value = EndpointAddr>`
error: could not compile `fsync-core` (lib) due to 3 previous errors
```
`core/fsync-core/Cargo.toml` and `node_addrs.rs` mtime 00:25:43 (dep added mid-edit). Debug `cargo test` had passed earlier. `cargo check -p fsync-core` passes now. Release build passes on snap2 and snap4.

### 4. e2e FAIL on snap4 (the one to look at)
Command: `env -u CARGO_TARGET_DIR bash tests/e2e/p0-exit-demo.sh` from snap4. Exit 1.
```
[e2e] paired, SAS words: rasi zoze zole
direct clip A->B latency_s=0.02 (A presence settle_ms=315, attempts=1)
direct file 256 MiB sha256=585c8de934079bcc send_secs=5.405 send_mib_s=47.36 recv_secs=7.105 recv_mib_s=36.03 via=Lan
[e2e] [relay-only] clipboard A -> B
[e2e] FAIL: [relay-only] clip watch failed: error: timed out
```
Relay-only round: `clip watch --timeout 90` timed out, no clip received. Relay-only clip latency was 0.28 s on snap2. Backend log has `connection ended with error error=read header from client timeout` near the end. Kept logs: `/var/folders/x8/d1lt6nyx39l2nxzvp3r6vt_m0000gn/T/tmp.bhgwWpRlxN` (script keeps them on failure).
Possible causes, not verified: relay URL reaching the backend late (phase 08b concern, relay-only latency ~20 s there); load from another session building in the live repo at the same time (`cargo build --release -p fsync-backend -p fsync-cli` plus a second e2e run in the live repo); or a regression in the presence/relay changes made 00:18 to 00:35. Not diagnosed.

## E2E timings

| Run | Round | Clip A->B | Presence settle | File send (MiB/s, secs) | File recv (MiB/s, secs) | Path | Result |
|---|---|---|---|---|---|---|---|
| snap2 (1st, only run there) | direct | 4.06 s | n/a | 47.37 MiB/s, 5.40 s | 36.40 MiB/s, 7.03 s | Lan | PASS |
| snap2 | relay-only | 0.28 s | n/a | 104.69 MiB/s, 2.45 s | 62.51 MiB/s, 4.10 s | Relay | PASS |
| snap4 (only run there) | direct | 0.02 s | 315 ms | 47.36 MiB/s, 5.41 s | 36.03 MiB/s, 7.10 s | Lan | PASS |
| snap4 | relay-only | timed out (90 s) | n/a | n/a | n/a | Relay | FAIL |

Flakiness: the same script passed on snap2 (after its 00:18 to 00:36 edits) and failed on snap4 on the relay round only. Direct-path numbers are stable (47 MiB/s send, 36 MiB/s recv in both runs). Re-run was not done (stop request), so flakiness is unconfirmed beyond this pair.

## Cleanup
- Ran `ps` and `docker ps -a` after the failed run: no backend, relay, spike, fsync-cli, or fsync-e2e processes or containers left from snap4. The script's own trap cleaned up.
- Pre-existing unrelated containers (mongodb, rabbitmq, redis, loki running; several Exited) were not touched.
- Another session's `cargo build --release -p fsync-backend -p fsync-cli` was running in the live repo during my snap4 e2e and is not mine; left alone.
- Scratch snapshots kept at `scratchpad/snap2`, `snap3`, `snap4` (no leftover processes).

## Deviations from spec
- `CARGO_TARGET_DIR` absolute paths were blocked by a repo hook that matches the word "target". Used the relative `target/test` value as specified.
- E2E was run with `CARGO_TARGET_DIR` unset (default). By inspection, the script's spike build (`cd spikes/iroh-discovery && CARGO_TARGET_DIR="$BUILD/spike"`) resolves a relative `BUILD` against the spike dir, so the later `[[ -x "$SPIKE" ]]` check would fail with a relative value. Not run to confirm.
- Did not run the ignored 1 GiB benchmark.

## Recommendations
1. Re-run the full gate set on a frozen tree (the final state), including e2e at least 3 times, because the relay-only round is intermittent.
2. Diagnose the relay-only clip timeout from `/var/folders/x8/d1lt6nyx39l2nxzvp3r6vt_m0000gn/T/tmp.bhgwWpRlxN/{backend,relay}.log` and the `round-relay-only/watch.err`.
3. Fix `scripts/`/`tests/e2e/p0-exit-demo.sh` relative `CARGO_TARGET_DIR` handling (resolve to absolute path before the spike build).
4. Keep the two new test files (FFI error mapping, CLI surface).

## Next steps (priority)
1. Final gate run on frozen tree (fresh tester).
2. Relay-only e2e failure triage.
3. Repeat e2e for flakiness.
4. Fix script relative-path bug.

## Unresolved questions
- Does the relay-only clip timeout on snap4 reproduce, and is it a regression from the 00:18 to 00:35 presence/relay edits or load from the concurrent session?
- Is the e2e script's relative `CARGO_TARGET_DIR` bug real? (Inspection only.)
- Which tree state is final? Live tree changed throughout the run.
