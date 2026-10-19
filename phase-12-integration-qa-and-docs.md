---
owns: ["docs/codebase-summary.md", "docs/project-changelog.md", "docs/development-roadmap.md", "apps/windows/README.md", "plans/261019-0016-desktop-ui-redesign-macos-windows/reports/qa-checklist-261019-desktop-ui-redesign.md"]
dependsOn: [05, 10]
---

# Phase 12: Integration, QA, docs (this Mac)
<!-- red-team #15: QA scoped to this redesign only -->

## Context Links
- All phase files; reports: `windows-test-baseline-<date>.md`, `windows-e2e-gate-<date>.md`, `windows-toast-spike-<date>.md`, `macos-settings-spike-<date>.md`
- Docs: `docs/codebase-summary.md` (says macOS 13), `docs/project-changelog.md`, `docs/development-roadmap.md`, `apps/windows/README.md`

## Overview
- Priority: P1. Status: pending. Depends on 05 and 10, plus 06/11 if they ran (add them to dependsOn before starting).
- Bring the Windows work from `rig-int/<slug>` into `feat/desktop-ui-redesign`, run the full matrix, write the QA checklist and docs.

## Key Insights
- No batch collect: every waso phase was already collected one by one (plan.md Remote loop). <!-- red-team #1 -->
- Supply-chain gate <!-- red-team #12 -->: before merging, review the cumulative diff `git diff feat/desktop-ui-redesign...origin/rig-int/<slug>` for any csproj, props, `Directory.*`, `nuget.config`, `packages.lock.json`, app.manifest, scripts or binary change. Any hit outside the plan → STOP and ask the user.
- Merge: `git fetch origin && git merge --no-ff origin/rig-int/<slug>` into feat. rig-int stays collect-only (never pushed to by hand).
- macOS 13 drop <!-- red-team #14 -->: release note in the changelog ("macOS 14 or later required; macOS 13 users stay on the previous release"); codebase-summary updated.
- macOS 14 untested (user accepted risk, Validation 1): QA checklist row "macOS 14: not verified" + release note line "tested on macOS 26". <!-- Updated: Validation Session 1 - macOS 14 untested -->
- QA covers only this redesign; rows from older plans are listed as a cross-plan note, not re-tested. <!-- red-team #15 -->

## Requirements
- QA checklist rows: macOS (split view, sections, Settings opener, popover, notifications, trust gate, drop confirm, retry) and Windows (left nav + footer Settings, Mica/fallback, trust gate on reopen, toasts or fallback, tray flyout, recovery, renamed titles), each with PASS/FAIL + evidence.
- Docs: codebase-summary (new shell, files, min macOS 14), changelog entry + release note, roadmap status, Windows README (nav, toasts, DLL note).
- Rollback note in the changelog entry: revert the merge commit on feat (Windows) and the macOS phase commits; no data migration involved (UI only, FFI unchanged).

## Architecture
```
rig-int/<slug> (07..11 collected) ─ supply-chain diff review ─ merge --no-ff ─▶ feat/desktop-ui-redesign (01..06 local)
   ─▶ macOS swift test + Windows test cmd + owner e2e ─▶ QA checklist + docs ─▶ PR to main
```

## Related Code Files
- Modify: `docs/codebase-summary.md`, `docs/project-changelog.md`, `docs/development-roadmap.md`, `apps/windows/README.md`
- Create: `reports/qa-checklist-261019-desktop-ui-redesign.md`
- Delete: none

## Implementation Steps
1. Supply-chain diff review; merge rig-int into feat (`--no-ff`).
2. macOS: `cd apps/macos/FsyncMac && swift build && swift test`; `cd apps/macos/Packages/FsyncCoreKit && swift test`.
3. Windows: `R run waso --ref <feat-sha> -- 'dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release'`; owner e2e scripts.
4. QA checklist; docs; roadmap/changelog.

## Todo List
- [ ] Supply-chain review + merge
- [ ] macOS + Windows test matrix
- [ ] QA checklist (this redesign only)
- [ ] Docs + release note + rollback note

## Success Criteria
- Both test matrices green (Windows: no failures outside the baseline list); owner e2e PASS on the merged sha.
- QA checklist has no FAIL in MVP rows.
- `grep -n 'macOS 13' docs/codebase-summary.md` empty; changelog has the release note.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Unreviewed dependency slips in | L x H | Cumulative diff gate before merge |
| Merge conflict feat vs rig-int | L x M | Disjoint trees (macOS vs Windows); resolve locally |
| macOS 13 users surprised | M x M | Release note + previous release stays available |

## Security Considerations
- No secrets in reports; DLL hashes only.

## Next Steps
- PR `feat/desktop-ui-redesign` → main. Follow-ups from plan.md.
