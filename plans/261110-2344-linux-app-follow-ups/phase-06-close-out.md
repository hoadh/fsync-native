# Phase 06 — Close-out

## Overview
- Priority P3. Estimate 0.5 d. Runs last.

## Steps
1. **Plan 261106** (`plans/261106-1414-linux-native-app/`):
   - Tick done items in phase 06 (Rig on waso-vm reinstalled 2026-11-10 with doctor ok; X2/X5 re-run at 30a6f3c; review + merge done as PR #1 / 4e7b33b).
   - Set phase statuses and plan `status: completed` once phase 05 here is done.
   - Fix the stale `200 lines` / `No CI workflow edits` notes where they no longer hold.
2. **Docs:** roadmap and changelog entries for this plan; update `docs/codebase-summary.md` if files were added.
3. **Devlog:** run `/rig:devlog` for the Linux app work (plan 261106 + this plan).
4. **Cleanup (each one asked first):**
   - Delete the merged `feat/linux-app` branch, local and remote.
   - Remove the 50 Rig runs on waso-vm older than 14 days (`rig-remote` cleanup, finished runs only).
   - Delete `C:\Users\dev\.claude\CLAUDE.md.rig1x.bak` once the user is happy with the new file.
5. Set this plan's `status: completed`.

## Todo
- [x] Plan 261106 closed (status completed 2026-11-17)
- [x] Roadmap, changelog (codebase summary unchanged: no new modules)
- [x] Devlog (`docs/devlog/`, four entries + INDEX)
- [x] Branch, waso-vm runs and backup file cleaned (asked 2026-11-17): 7 Linux branches deleted local + remote; 51 finished waso-vm runs > 14 d removed (5 left); `CLAUDE.md.rig1x.bak` was already gone; local worktrees/caches and LIVE test files (host + GNOME VM) removed
