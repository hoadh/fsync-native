# Merged main verification (a2c9a2d)

Scope: HEAD a2c9a2d (Track R PeerRefused + Track N3 test-relay-ca/QAD). No source edits, no commits.

## Results

| # | Step | Result | Wall | Counts |
|---|------|--------|------|--------|
| 1 | cargo fmt --check | PASS | <1s | clean |
| 2a | clippy --workspace --all-targets -D warnings | PASS | 43s | 0 warnings |
| 2b | clippy fsync-core test-relay-ca | PASS | 15s | 0 warnings |
| 2c | clippy fsync-cli test-relay-ca | PASS | 17s | 0 warnings |
| 3a | cargo test --workspace, run 1 | PASS | 354s | 360 passed, 0 failed, 3 ignored |
| 3b | cargo test --workspace, run 2 | PASS | 158s | 360 passed, 0 failed, 3 ignored |
| 3c | relay_private_ca (test-relay-ca) | PASS | 18s | 2/2 |
| 4a | scripts/build-xcframework.sh (incl. feature guard) | PASS | 109s | universal x86_64+arm64 written |
| 4b | swift test FsyncCoreKit | PASS | 25s | 14 passed, 0 failures |
| 4c | swift test FsyncMac | PASS | 39s | 231 executed, 18 skipped (live API), 0 failures |
| 5 | p0 e2e (MIB=16) | PASS | 111s | p0 local, clip latency 0.02s |
| 6 | netns strict (NETNS_MIB=64) | FAIL | 262s | PASS=9 FAIL=1 QUARANTINED=0 SKIP=1 |

Flakiness: none. Both workspace runs identical. No single-test reruns needed.

## netns detail

| Scenario | Gating | Result | Path | Clip / note |
|---|---|---|---|---|
| n1 | y | PASS | lan | clip 0.01s |
| n2 | y | PASS | lan | clip 0.02s |
| n5 | y | PASS | relay | clip 0.01s |
| n3 | y | PASS | direct | clip 0.01s |
| n3b | y | PASS | direct | clip 0.01s |
| n4 | y | PASS | relay | clip 0.01s (< 1.5s gate) |
| n7 | y | PASS | - | clip 0.01s |
| n9 | y | PASS | lan | clip 0.02s |
| n6 | n | PASS | - | 1 GiB re-address |
| n8 | y | **FAIL** | - | see below |
| sec | n | SKIP | - | expected (CLI cannot dial unpaired NodeId) |

Result JSON: `/Users/alex/Code/fsync-native/docs/test-results/netns-261017-1815.json` (uncommitted, untracked).
Netns cell log (failure): `/var/folders/x8/d1lt6nyx39l2nxzvp3r6vt_m0000gn/T/tmp.y79nWeOdgr/netns-local.log`

## Failure: n8 (deterministic, not flaky)

- Assertion in `tests/e2e/scenarios/n8-reset-trust.sh`: B's clip put and file push must fail with `not paired with that device`.
- Actual: `error: the other device removed this one`.
- Cause: Track R (`0a9e1c8`) intentionally maps removed-by-peer (403 close) to `FsyncError::PeerRefused`, message "the other device removed this one". `docs/ffi-contract.md` L100/L205 and `docs/system-architecture.md` L220 document this. `NotPaired` is now only for no-pairing-at-all.
- Verdict: the product behavior is the intended new behavior. The N8 scenario expectation is stale. Refusal itself works (B is refused, A receives nothing). Still a gating FAIL under strict mode, so the matrix is red.
- Fix direction (not applied, per instructions): update the grep in n8-reset-trust.sh (both clip loop and file push) to `the other device removed this one`, or match either string.

## Environment notes

- HEAD moved during the run. Four Windows commits landed on main at 18:00-18:04 (`1d65725`, `9f68daa`, `7172cd7`, `9071a9d`), from outside this session. Diff a2c9a2d..HEAD = only `OleClipboard.cs` and `Win32Clipboard.cs` (C#). No Rust, Swift, or e2e input changed, so results above still apply to Rust/Swift/e2e.
- The netns JSON carries `git_sha: 9071a9d-dirty` (computed at run time), not a2c9a2d. Dirty flag is from untracked `.claude/`.
- Other Docker containers (suspicious_hopper, mongodb, rabbitmq, redis, loki) were not touched. `mystifying_euclid` (exited) untouched. No leftover fsync-netns-* containers. Cache volumes `fsync-netns-{cargo,rustup,build}` kept by design.
- xcframework build left no tracked diffs in git status.

## Critical issues

1. netns N8 gate red due to stale assertion after Track R. Blocks a green strict netns run and the committed results JSON (currently records FAIL).

## Recommendations

1. Update N8 expected string to the PeerRefused text, rerun `NETNS_STRICT=1 NETNS_MIB=64 tests/e2e/run-matrix.sh --scenarios netns` (or `NETNS_SCENARIOS=n8,...`), then commit the new JSON.
2. Consider a Rust-side guard test that the removed-peer message is the PeerRefused text, so e2e and FFI docs don't drift again.
3. Re-verify if further commits land on main (HEAD drifted during this run).

## Next steps

1. Fix n8 assertion (test owner).
2. Rerun netns strict to get all-green JSON.
3. Decide whether the 4 Windows commits need Windows-side verification (not in this run's scope; Swift/Rust unaffected).

## Unresolved questions

- Should N8 be expected to report `PeerRefused` on the CLI, or is the N8 intent (typed refusal) satisfied by any refusal string? Owner call.
- Who committed the Windows clipboard commits to main mid-run, and is that intended? Not verified.

**Status:** DONE_WITH_CONCERNS
**Summary:** fmt, clippy (3 variants), workspace tests (x2, stable), relay_private_ca, xcframework guard, Swift tests, and p0 e2e all pass. netns strict run fails on one gating scenario (n8): stale grep expects old NotPaired text, product now correctly returns PeerRefused.
**Concerns/Blockers:** n8 gate red; HEAD moved mid-run (4 Windows C# commits, no effect on tested Rust/Swift/e2e inputs); netns JSON provenance shows 9071a9d-dirty.
