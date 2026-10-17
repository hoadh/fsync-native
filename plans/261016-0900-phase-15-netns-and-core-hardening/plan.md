---
title: "Phase 15 netns matrix (Track N) + core hardening (Track K)"
description: "Local Linux-netns network matrix N1-N9 in a privileged Docker container, plus fetch retry, engine.lock closure and CLI path tier in core."
status: completed
priority: P1
effort: 3.5d
branch: main
tags: [e2e, netns, nat, core, iroh, cli, hardening]
created: 2026-10-16
updated: 2026-10-17
parent: ../261013-2029-fsync-native-mvp/plan.md
base: main 68d7ae6
---

# Goal
Close MVP phase 15 todo "netns lib + N1/N2/N5" and "N3/N4/N6-N9" locally, and close the core gaps from round 2:
four-track Next action 10 (fetch retry part) and 12 (netns part); Unresolved 7 (fetch retry lands now). Unresolved 5 (relay mesh) stays deferred.

# Phases
| Phase | Name | Status |
|---|---|---|
| [01](phase-01-track-n-netns-matrix.md) | Track N: netns matrix (N1-N9, best-effort security) | Completed (N3, N4 un-quarantined 10-17; strict netns green c429406) |
| [02](phase-02-track-k-core-hardening.md) | Track K: fetch retry, engine.lock closure, CLI path tier + tracing | Completed |
| [03](phase-03-n8-sender-refusal-feedback.md) | N8: sender told when refused (clip/2 ack; FFI `PeerRefused` 10-17) | Completed (strict netns green c429406) |

Progress: 3/3 phases complete. Open gate: netns strict rerun on merged main.

# Status (2026-10-17, local main, not pushed)
- Track N3 (a2c9a2d): feature-gated `test-relay-ca` hook, relay TLS + QAD in netns harness, private-CA test, build guards (xcframework, Windows DLL). Results: N3 direct clip 0.01 s; N4 relay clip 0.01 s (was ~4 s; cause: iroh net report waiting ~3 s for QAD on a QAD-less relay, not the dialer).
- Track R (merged): FFI `FsyncError::PeerRefused`, `Published.refused`, CLI/Mac/Windows refusal texts. Review 9.5. Closes phase-03 open item.
- n8 e2e assertion updated to PeerRefused text.
- Unplanned Windows clipboard fix (found in verification): OLE clear/flush retry `CLIPBRD_E_CANT_OPEN` while rdpclip holds the clipboard; ownership re-checked per attempt; reader open budget 250 ms. Commits 1d65725, 9f68daa, 7172cd7, 9071a9d. Review 9.5.

# Verification on merged main
- `cargo fmt --check`, clippy incl. test-relay-ca: clean.
- `cargo test --workspace` x2: 360 pass, 0 fail, 3 ignored. `relay_private_ca` 2/2.
- xcframework build + guard OK. FsyncCoreKit 14/0. FsyncMac 231 run, 18 skipped, 0 fail.
- p0 MIB=16 PASS (clip 0.02 s). Windows dotnet 158/158 (9+47+102), interactive session.
- netns strict on c429406 (tree dirty with plan markdown only): PASS=10 FAIL=0 QUARANTINED=0 SKIP=1 (sec), rc 0, 259 s, `docs/test-results/netns-261017-1821.json`.

# Scope changes (deviation, reason, impact)
- K `FIRST_ATTEMPT` 2 s -> 5 s; regression bound 10 s -> 20 s. Reason: slow relay (~0.4 MiB/s). Owner signed off 2026-10-17.
- K redial-stall cleanup closes peer's idle, then in-use inbound clip/presence conns (pulls spared); they reconnect on use. Owner signed off 2026-10-17.
- K loopback counted as `lan`; `fetched` tier key is `path_tier`. `via` unchanged.
- N2/N9 pass via K backend-down start. n3b added. `cli_path` filled from K fields.
- N8: 026fb5a reused `NotPaired`; superseded 10-17 by FFI `PeerRefused` (additive, appended last). Impact: Mac and Windows must rebuild.
- N3 `test-relay-ca`: test-only feature, guarded out of FFI, xcframework and DLL builds. No prod effect.
- Windows clipboard fix: unplanned, no spec change, no FFI change.

# Out of scope (deferred, unchanged)
Relay mesh decision; A5 mid-run relay death on real relays; UI "Direct" label; real Mac/Win cross-OS runners; nightly CI; 1 GiB file sizes.

# Next actions (owner, definition of done)
1. Done (c429406, netns-261017-1821.json). Lead: run netns strict on merged main. DoD: JSON in `docs/test-results/`, n8 PASS, strict exit 0, no fsync-netns containers left.
2. Lead: commit netns JSON (untracked); push local main after gates. DoD: pushed, one green GitHub CI run.
3. Owner: decide prod relays `RELAY_QAD=0` -> enable QAD (needs UDP 7842 open in Lightsail firewall; cert readable without `RELAY_USER=0`). DoD: decision recorded; if yes, N3 direct verified on a prod-like relay.
4. Mac and Windows app owners: rebuild with clip/2 + `PeerRefused`; wire `send_files` and `Published` into UI; check refusal text on both OS. DoD: manual check recorded in QA checklist.
5. Windows ARM64 DLL build. DoD: build green or owner defers.
6. Lead docs impact: minor. `docs/ffi-contract.md` (PeerRefused additive), CLI `path`/RUST_LOG, changelog.

# Unresolved questions
1. Enable QAD on prod relays (`RELAY_QAD=0` today)? Owner decision.
2. Mixed-version clip/1 vs clip/2 peer: core test covers fail-fast; not tried live against an old app build.
3. Refusal text does not name the device. Keep generic or name it?
4. Optional hardening: freeze legacy mtime as stamp; move stamp fsync off async thread.
5. netns JSON files untracked. Commit before push (lead).
