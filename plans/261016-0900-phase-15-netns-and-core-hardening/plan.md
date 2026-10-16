---
title: "Phase 15 netns matrix (Track N) + core hardening (Track K)"
description: "Local Linux-netns network matrix N1-N9 in a privileged Docker container, plus fetch retry, engine.lock closure and CLI path tier in core."
status: pending
priority: P1
effort: 3.5d
branch: main
tags: [e2e, netns, nat, core, iroh, cli, hardening]
created: 2026-10-16
parent: ../261013-2029-fsync-native-mvp/plan.md
base: main 68d7ae6
---

# Goal
Close MVP phase 15 todo "netns lib + N1/N2/N5" and "N3/N4/N6-N9" locally, and close the core gaps from round 2:
four-track Next action 10 (fetch retry part) and 12 (netns part); Unresolved 7 (fetch retry lands now). Unresolved 5 (relay mesh) stays deferred.

# Tracks (run in parallel, owner approved)
| Phase | Name | Status |
|---|---|---|
| [01](phase-01-track-n-netns-matrix.md) | Track N: netns matrix (N1-N9, best-effort security) | Pending |
| [02](phase-02-track-k-core-hardening.md) | Track K: fetch retry, engine.lock closure, CLI path tier + tracing | Pending |

| Track | Branch / worktree | Owns (only) | Effort |
|---|---|---|---|
| N | `track-n/netns-matrix`, `../fsync-native-track-n` | `tests/e2e/**`, `docs/test-results/**` | 2.5d |
| K | `track-k/core-hardening`, `../fsync-native-track-k` | `core/**` (no `apps/**`; FFI additive only, none planned) | 1d |

Lead owns `plans/**`, `docs/**` (except `docs/test-results/**`), `README.md`, merges.

# Dependencies
- N and K are independent: disjoint paths, N builds the CLI from its own worktree (main 68d7ae6).
- N must NOT depend on K's CLI `path` field; path is asserted from veth byte counters. After both merge, lead runs one netns pass on merged main; the CLI `path` (if present) is logged as a cross-check only.
- Merge order: any. No shared files, so no rebase conflicts expected.

# Gates (per track, before lead merges)
- code-reviewer >= 9.5, 0 critical.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- N: one green warm netns run < 15 min, metrics JSON committed in `docs/test-results/`; `p0-exit-demo.sh` (MIB=16) still PASS if p0 was refactored.
- K: new regression test fails on main and passes on branch (show both); `p0-exit-demo.sh` MIB=16 PASS (CLI output stays compatible); `scripts/build-xcframework.sh` + `swift test` green (core behavior changed under the apps).

# Out of scope (deferred, unchanged)
Relay mesh decision; A5 mid-run relay death on real relays; UI "Direct" label (apps + FFI); real Mac/Win cross-OS runners (lab hardware); nightly CI workflow; 1 GiB file sizes.

# Environment rules (both tracks)
- macOS arm64, Docker Desktop Linux aarch64 8 GB. Never use the shared VPS hosts or hosted relays.
- Shell hook blocks the literal word for cargo's output dir: write `T="tar""get"` and use `$T`.
- No `timeout` on the Mac host (use the watchdog pattern in `run-matrix.sh`); inside the Linux container coreutils `timeout` is fine.
- Never touch Docker container `mystifying_euclid`. Only create/remove resources named `fsync-netns-*` / labeled `fsync.e2e=netns`. No `docker system prune`, no wildcard rm.
- No force-push; conventional commits; no AI references.

# Rollback
- N: purely additive scripts + one results JSON; revert merge commit. Docker volumes `fsync-netns-cargo`, `fsync-netns-build` removable by name.
- K: each item is its own commit; revert individually. No schema, FFI or on-disk format change.

# Docs impact (lead, at merge)
Minor: MVP phase-15 todo ticks + results link; phase-08 line 90 (M1/M2 already fixed, see phase 02); `docs/` CLI output (`path` field, RUST_LOG); changelog.

# Unresolved questions
1. QAD in the local relay: client trusts webpki roots only, so a local relay likely has no QUIC address discovery. If N3 cannot hole-punch without it, accept N3 = "quarantined, finding filed" or fund a core test hook (extra CA via env) in a later round?
2. N2/N9 need `ensure_account` to work with backend unreachable. If it does not, that is a product finding: quarantine or fix in K (scope add)?
3. Windows `dotnet` run on waso for K: required? No FFI change, so proposed optional.
