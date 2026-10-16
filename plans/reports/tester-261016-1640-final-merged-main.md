# Final verification: local main @ 026fb5a

Scope: Track K core hardening, Track N netns matrix, CLI send fix, N8 clip/2 refusal ack. No source edits, no commit, no push.

## Results

| Step | Check | Result |
|---|---|---|
| 1 | `cargo fmt --all --check` | PASS (rc 0) |
| 1 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| 1 | `cargo test --workspace` run 1 | PASS 354, FAIL 0, ignored 3 (384 s, cold) |
| 1 | `cargo test --workspace` run 2 | PASS 354, FAIL 0, ignored 3 (151 s) |
| 2 | `scripts/build-xcframework.sh` | PASS, universal x86_64+arm64, 0 warnings |
| 2 | `swift test` FsyncCoreKit | PASS 14, FAIL 0 |
| 2 | `swift test` FsyncMac | PASS 231 executed, 18 skipped, FAIL 0 |
| 3 | `MIB=16 run-matrix.sh --scenarios p0` | PASS (clip A->B 0.02 s) |
| 4 | `NETNS_STRICT=1 NETNS_MIB=64 ... --scenarios netns` | rc 1 (expected, strict gating on n3/n4) |
| 4 | `NETNS_MIB=64 ... --scenarios netns` (non-strict) | rc 0 |

Ignored core tests are diagnostic or benchmark only (interface verdict print, 1 GiB loopback bench, 200 MiB bench).
Ignored Swift tests are the 18 FsyncMac integration tests that need `FSYNC_IT_API`.

## Netns matrix

Strict run: `docs/test-results/netns-261016-1643.json`
Non-strict run: `docs/test-results/netns-261016-1649.json`

Both runs gave the same result set (git_sha `026fb5a-dirty`, dirty flag from untracked files only):

| id | result | gating | note |
|---|---|---|---|
| n1 | PASS | yes | |
| n2 | PASS | yes | |
| n5 | PASS | yes | |
| n3b | PASS | yes | |
| n6 | PASS | no | re-address mid-transfer, 1024 MiB |
| n7 | PASS | yes | B offline after 3 s |
| n8 | PASS | yes | 3/3 clip puts and file push refused with typed error, A stored nothing |
| n9 | PASS | yes | |
| n3 | QUARANTINED | yes | no direct path behind full-cone NAT, no QUIC address discovery on test relay |
| n4 | QUARANTINED | yes | relay path OK, clip latency 4.03 s vs 1.5 s spec |
| sec | SKIP | no | unpaired NodeId dial not expressible via CLI, covered by core tests |

Counts: PASS 8, FAIL 0, QUARANTINED 2, SKIP 1. Matches expected. Strict rc 1 is caused only by n3/n4 gating.

## Docker cleanup

- No `fsync-netns-*` containers or networks left.
- Only cache volumes remain: `fsync-netns-cargo`, `fsync-netns-rustup`, `fsync-netns-build`.
- `mystifying_euclid` untouched (Exited).

## Concerns (non-blocking)

- Swift warnings in FsyncMac tests: non-Sendable `NSPasteboard` capture in `@Sendable` closure (`FileClipWriterTests.swift:88,90`), unnecessary `nonisolated(unsafe)`, FocusState read outside body (`OnboardingRenderTests.swift:25`).
- FsyncMac integration tests (18) not run. They need `apps/macos/scripts/run-fsync-mac-integration.sh` and a live dev API.
- n3/n4 quarantine means the strict gate is red on main by design. Known gap, not a regression from this merge.

## Unresolved questions

1. n6 reports 1024 MiB while `NETNS_MIB=64`. Is n6 size hardcoded or intentionally fixed? Gating is False, so it does not affect the result.
2. Should the FsyncMac integration suite be run against the dev backend before tagging?
3. Should the `-dirty` git_sha flag be cleaned up? It comes from untracked `.claude/` and report files, not tracked code.

**Status:** DONE_WITH_CONCERNS
**Summary:** 026fb5a passes fmt, clippy, core tests (354 x2), xcframework build, Swift tests, p0, and netns (8 PASS, 2 QUARANTINED, 1 SKIP). Strict rc 1 is expected from n3/n4. Non-strict rc 0. No leftover fsync-netns containers or networks.
**Concerns/Blockers:** Swift test warnings, 18 unrun FsyncMac integration tests, and the n3/n4 strict-gate quarantine (known).
