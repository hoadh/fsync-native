# Docs maintainer report: Linux app docs (phase 06 step 7)
Date: 2026-11-09. Branch `feat/linux-app`. Docs only, no code change, nothing committed.

## Status
DONE_WITH_CONCERNS. Docs updated from code and result files. ADR 014 kept `draft` (X3/X4 pending).
Concerns below need owner decisions.

## Current state assessment
- Doc layout is legacy (`docs/system-architecture.md`, `development-roadmap.md`, `project-changelog.md`,
  `codebase-summary.md`). No Diataxis dirs exist. Updated in place, no restructure (per instruction).
- ADR 014 already had "Platform emit gate (future)" and "Untested desktops" sections; both kept and extended.
- Clipboard lifecycle policy already in ADR 014 ("Lifecycle policy (v1)"); not duplicated.
- Stale before this run: roadmap and changelog last updated 2026-10-17; codebase-summary 2026-10-17; no Linux content anywhere except ADR 014 and `apps/linux/README.md`.
- Plan `plan.md` phase 05 said "Completed" while `phase-05` file says In Progress. Followed phase file.

## Changes made
| File | Change |
|---|---|
| `docs/system-architecture.md` | Header date; overview mentions Linux; new `## Linux app` section (Rust core + CXX-Qt/QML, platform crate, clipboard backends incl. GNOME manual mode, secrets oo7 + 0600 file fallback, single-instance flock + IPC socket, eager inbox, `Platform::Cli` on the wire); ADR range 001-014; Linux known-gap bullet. |
| `docs/development-roadmap.md` | Header date; new `## Linux Desktop App` phase table (01 Done; 02-06 In progress with open LIVE items; X1/X2/X5 PASS, X3/X4 pending); two milestones. |
| `docs/project-changelog.md` | New entry 2026-11-09: Linux app + packaging (deb/rpm/AppImage/PKGBUILD), core `BUILT_FEATURES`, harness, tolerant `Platform` codec, upgrade note, verification. |
| `docs/codebase-summary.md` | Header date; repo-map rows for `apps/linux/{app,platform,packaging,scripts,tests/e2e}`; rows for `apps/windows/tests/e2e/cross-os-cli-peer.ps1` and `cross-os-port-forward.ps1`; test row for `apps/linux/platform/tests`. All `apps/` paths checked to exist. |
| `README.md` | Intro mentions Linux; Qt prerequisite row; `## Linux desktop app` build/run block copied from `apps/linux/README.md` (commands unchanged, pointer to it); layout row. |
| `docs/adr/014-linux-desktop-app.md` | Status line: stays draft until X3/X4. Platform emit gate: current state (gate closed, `emit-platform-linux` off, codec in `fsync-proto`, no backend or Mac/Windows deploy recorded). New `## Cross-OS results (phase 06)`: 12-leg table from `linux-cross-os-261109-1753.json`, deviations, known issues (two pairing flakes), open list. Replaced the old `## Open` stub. Untested desktops list kept as is. |
| `docs/test-results/README.md` | Pointer to `linux-cross-os-*.json` format and latest run. |
| `plans/261106-1414-linux-native-app/plan.md` | Phase 05 -> In Progress (matches phase file); phase 06 -> In Progress; progress 1/7. |
| `plans/261106-1414-linux-native-app/phase-06-cross-os-test-and-docs.md` | Status line; todo: docs done, ADR final and emit gate open. |

Files changed by this report: the 9 above plus this report file.

## Verification
- Every referenced file path exists (manual check of `.md`, `.sh`, `.ps1`, `.json` links and `apps/` paths).
- Anchors added: `#linux-app`, `#platform-emit-gate-future`, `#cross-os-results-phase-06` match headings.
- `node ~/.claude/scripts/validate-docs.cjs docs/`: no broken links; only legacy-layout warnings (kept on purpose) and code-reference noise from pre-existing ADRs.
- Numbers copied from `docs/test-results/linux-cross-os-261109-1753.json` (1753 run; the 1628 and 1741 runs are cited as earlier runs). Text range in changelog 51-2973 ms; file range 2.1-4.5 s.
- ADR 014 is 557 lines, under `docs.maxLoc` 800.
- Concurrent on-disk edits were seen to roadmap, changelog, codebase-summary, README, plan.md and phase-06. My edits were applied on top; `git diff` shows only my hunks.

## Gaps identified
1. X3 and X4 (Windows GUI, interactive taskMode) not run; roadmap and ADR say pending.
2. Hyprland KeePassXC / Bitwarden LIVE check, GNOME LIVE items from 02/04a/04b, Omarchy `makepkg -si` and autostart: not done.
3. Remaining Platform emit gate prerequisites (backend deploy, Mac/Windows releases) not evidenced.
4. Pairing flakes: root cause unknown.
5. No cross-OS run has put `Platform::Linux` on the wire (gate closed by design). The codec is covered by `core/fsync-proto/tests/platform_codec.rs` (commit c2d1e4f) only.
6. `apps/linux/README.md` not changed in this run. Its "Clipboard" section still says Hyprland/Sway/KDE/COSMIC are auto (accurate per ADR detection, but KDE/Sway live are untested per ADR 014). Left for a follow-up.

## Recommendations (priority order)
1. Owner decision on the 30 s gate for Windows file legs (see Unresolved questions). Then ADR 014 accepted once X3/X4 pass.
2. Run X3/X4 with `taskMode: interactive` (or the manual checklist), then restore `taskMode` on waso-vm.
3. Confirm no tunnel process or portproxy rule remains (plan step 8), then code review and merge.
4. Root-cause the pairing flakes (TCP-only relay path is the suspect; not confirmed).
5. Migrate to Diataxis layout in a separate task (`/rig:docs restructure`), not in this branch.
6. Repomix compaction and the codebase summary: see Unresolved questions.

## Metrics
- Docs touched: 9 files. ADR 014: 489 -> 557 lines. Roadmap, changelog and codebase summary: stale since 2026-10-17, now dated 2026-11-09.
- Coverage: Linux app has architecture, roadmap, changelog, codebase-summary, README and ADR entries. Open LIVE items are listed, not hidden.

## Unresolved questions
1. **File-leg gate.** The plan's success criteria say "latencies <= 5 s". The harness gates Windows file legs at 30 s (measured 3.2-4.5 s; one 5014 ms run failed 5 s). Keep 30 s for files over the tunnel, or change the harness? Docs record 30 s as a pending owner decision.
2. **Transport wording.** Plan phase 06 (Architecture, step 2, Key Insights) still describes `netsh portproxy` / `ssh -L`. Implementation uses a reverse `ssh -R` tunnel per run. ADR 014 follows the code. Should the phase text be corrected?
3. **Repomix compaction not generated.** The repomix shim failed (mise version error), and the fallback `npx` fetch was denied by the auto-mode classifier. The Diataxis path `docs/reference/codebase-summary.md` does not exist in this repo; the legacy `docs/codebase-summary.md` was updated by hand from the repo tree and the files read. Rerun repomix in an approved environment if a fresh compaction is required.
4. **Pairing flakes.** Are the two offer/join failures a product bug (fresh endpoint not addressable via relay) or a harness timing issue? Not determined.
5. **Phase 05 status.** Plan overview said Completed; phase file says In Progress. This report follows the phase file; confirm.
6. **Platform emit gate deploy.** Has the backend tolerant decoder been deployed to production? No record found in this plan; ADR says not yet.

## Status
DONE_WITH_CONCERNS: docs aligned with code and results; ADR 014 remains draft; owner decisions in Unresolved questions 1, 2 and 5.
