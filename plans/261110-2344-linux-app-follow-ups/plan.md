---
title: "Linux app follow-ups after the PR #1 merge"
description: "Pairing flake root cause, lazy-file latency, offline-publish lows + FFI contract bump, packaging hardening, LIVE checks, close-out of plan 261106."
status: pending
priority: P2
branch: fix/linux-app-follow-ups (one branch per phase, PR each)
tags: [fix, linux, core, packaging, test]
blockedBy: []
blocks: []
created: 2026-11-10
---

# Goal
Close every open item left by [plan 261106-1414](../261106-1414-linux-native-app/plan.md) after PR #1 (merged as 4e7b33b): the open questions, the review lows, and the LIVE checks. Then mark that plan completed.

Inputs:
- ADR 014 "Known issues" and "Open": `docs/adr/014-linux-desktop-app.md`.
- Reviews in `../261106-1414-linux-native-app/reports/`:
  - `code-reviewer-261110-1700-offline-peer-publish-rereview.md` (L1, L2)
  - `code-reviewer-261110-1830-offline-peer-publish-rereview-2.md` (L-new-1)
  - `code-reviewer-261107-1700-phase-05.md` (L3-L6, L8)
- Evidence:
  - `implementer-261109-2010-phase-06-x3-x4.md`
  - `docs/test-results/linux-cross-os-261110-1241.json` and `-1511.json`

# Phases
| Phase | Name | Status |
|---|---|---|
| [01](phase-01-pairing-flake-root-cause.md) | Pairing "connection lost" flake: reproduce, root-cause, fix (3 d) | Pending |
| [02](phase-02-offline-publish-lows-and-ffi-contract.md) | Offline-publish L1/L2 (latest slot per lane, serialized ordering doc) + `contract_version` 3 (1 d) | Pending |
| [03](phase-03-lazy-file-latency.md) | Linux -> Windows lazy file 8.7 s vs 10 s gate: measure, cut the wait (1.5 d) | In Progress |
| [04](phase-04-packaging-hardening.md) | PKGBUILD git sha, Phase 05 lows L3/L4/L6/L8, maintainer, repo license MIT OR Apache-2.0 (1.5 d) | Pending |
| [05](phase-05-live-checks.md) | LIVE checks on Hyprland + GNOME VM; tunnel-leftover check (1 d, user at the desktop) | Pending |
| [06](phase-06-close-out.md) | Devlog, plan 261106 close-out, branch + waso-vm run cleanup (0.5 d) | Pending |

Estimate: about 8.5 working days.

# Dependencies
- 01 and 02 both touch `core/fsync-core/src/clip/` and `net/`, so run them one after the other: 01 first, because it may change the dial path that 02's lanes sit on.
- 03 needs waso-vm (X3 leg, `taskMode` interactive) and a user logged on over RDP. Run it after 01 so the X3 rerun also checks pairing.
- 04 is independent (packaging only). It can run in parallel with 01-03 on its own branch.
- 05 is manual and needs the user. Run it after 01-03 so the LIVE checks cover the final code.
- 06 runs last.

# Gates
- **Flake fixed** (01): 20 consecutive full local pairing runs (`apps/linux/tests/e2e` pairing leg) without "connection lost", against the 1-in-12 baseline (about 20 % chance of 20 clean runs if nothing changed). Plus one X1+X2+X5 cross-OS run.
- **Contract bump** (02): the Swift and C# bindings are regenerated, and macOS and Windows CI are green. The Windows app (already) and the macOS app (new) check `contract_version() == 3` at startup.
- **Lazy file** (03): 5 X3 runs with app→win file at or under 5 s median and under 10 s max.
- **Packaging** (04): `smoke-install.sh` passes for all targets, and arch shows namcap with no errors.
- Each phase: tester + code review >= 9.5 with 0 critical, then a PR. CI is green, including `stack`. The user merges, or I merge when told to.

# Cross-cutting rules
- Files < 500 lines (user CLAUDE.md; the older 200-line rule in `docs/code-standards.md` applies to new files where practical).
- Never touch the user's real clipboard, keyring, Hyprland session or the installed `/usr/bin/fsync-linux` data dir/socket in automated runs (phase 05 is the user doing it by hand).
- Rig on waso-vm is now 2.0.3. Use `rig-remote` from the plugin; the 1.x backup script is retired. `taskMode` interactive only for the GUI run, restored to headless after.
- Never log secrets, tokens, clipboard content or file names.

# Success (whole plan)
- ADR 014 "Known issues" has no unexplained flake, and "Open" holds only deliberate gaps (X4 QR, Platform emit gate).
- Plan 261106 and this plan are both `completed`. Roadmap and changelog are updated.

# Unresolved questions
1. License: answered 2026-11-11: open source, my choice. Chosen: `MIT OR Apache-2.0` (Rust convention; compatible with every license in `apps/linux/about.toml`).
2. Maintainer: answered 2026-11-11: `alex <danghuyhoa@gmail.com>` (public in the packages).
3. Phase 05 L5: answered 2026-11-11: deferred to MVP phase 16 (release/signing).

No open questions remain.
