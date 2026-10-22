---
title: "Desktop UI redesign: macOS (SwiftUI) and Windows (WinUI 3)"
description: "Native-feeling redesign of both desktop apps on a shared IA: macOS sidebar main window + Settings window, Windows Left nav + Mica + parity features."
status: in-progress
priority: P2
branch: feat/desktop-ui-redesign
tags: [frontend, feature, macos, windows]
blockedBy: []
blocks: []
created: 2026-10-19
orchestrator: alexpro
---

# Desktop UI redesign (macOS + Windows)

## Overview
Windows IA (Devices, History, Files, Pair, Settings) is the reference; each platform renders it natively. macOS: main Window with NavigationSplitView, pairing sheet, Transfers page, Settings window, restyled popover, file notifications. Windows: Left nav, footer Settings, Mica, title bar, InfoBar, empty states, toasts, trust-reset + recovery parity. No core/FFI change. Context: `reports/explore-261019-ui-inventory-macos-windows.md`, `research/`. Cross-plan (related, not blocking): `261013-2029-fsync-native-mvp` 12/13 and `261015-0410-four-track-hardening` C4/B manual QA stay in their own plans (not in this merge gate). <!-- red-team #15 -->

## Phases
| Phase | Name | Status |
|---|---|---|
| 01 | [Shared design spec](phase-01-shared-design-spec.md) | Completed |
| 02 | [macOS core models + target bump](phase-02-macos-core-models-and-target-bump.md) | Completed |
| 03 | [macOS pages](phase-03-macos-pages-devices-history-transfers-pairing.md) | Completed |
| 04 | [macOS shell, Settings window, popover](phase-04-macos-shell-settings-window-and-popover.md) | Completed |
| 05 | [macOS incoming-file notifications](phase-05-macos-incoming-file-notifications.md) | Completed |
| 06 | [macOS stretch (optional)](phase-06-macos-stretch-quicklook-commands-icon-states.md) | Skipped |
| 07 | [Windows core VMs: trust, recovery, notices](phase-07-windows-core-viewmodels-trust-recovery-notices.md) | Completed |
| 08 | [Windows shell + e2e helpers](phase-08-windows-shell-mica-titlebar-nav-e2e-helpers.md) | Completed |
| 09 | [Windows trust gate, toasts, tray menu](phase-09-windows-trust-gate-toasts-tray-menu.md) | Completed |
| 10 | [Windows pages + recovery UI](phase-10-windows-pages-polish-and-recovery-ui.md) | Completed |
| 11 | [Windows stretch (optional)](phase-11-windows-stretch-drop-countdown-first-device.md) | Skipped |
| 12 | [Integration, QA, docs](phase-12-integration-qa-and-docs.md) | Pending |

Open items (not gating): 05 manual bundle smoke (owner). 04 manual `swift run` smoke (owner). 06 skipped by owner. 08 min window size deferred (Q2). Phase 10 must re-check "no page title equals nav name".

| Phase | Machine | Owns (exact list in frontmatter) | dependsOn | Test |
|---|---|---|---|---|
| 01 | local | spec, ADR 003 | - | review only |
| 02 | local | Package.swift, bundle script, listed FsyncMacCore files + tests | 01 | mac |
| 03 | local | FsyncMacUI Devices/History/Transfers/Pairing/Onboarding, TrustResetPrompt | 02 | mac + kit |
| 04 | local | FsyncMacApp.swift, SettingsOpener, FsyncMacUI root/Shell/Settings/Menu | 03 | mac |
| 05 | local | AppDelegate.swift, Notifications/** + test | 04 | mac |
| 06 | local (optional) | new stretch files + documented handoff edits | 05 | mac |
| 07 | waso | listed Fsync.App.Core files + Fsync.App.Tests files | 01 | win |
| 08 | waso | App.xaml(.cs), MainWindow.*, Styles/Shell, Dialogs/Ui, tests/e2e/* | 07 | win + e2e gate |
| 09 | waso | App.Parity.cs, MainWindow.TrustGate.cs, Notifications/**, TrustResetPage.*, TrayHost.cs, Program.cs | 08 | win |
| 10 | waso | 6 pages (xaml + cs), Views/Controls/** | 09 | win |
| 11 | waso (optional) | page partials *.Drop/Countdown/Extras.cs, DropPaths, FirstDevice, PairingViewModel | 10 | win |
| 12 | local | docs, Windows README, QA checklist | 05, 10 (06, 11 if run) | mac + kit + win + e2e |

- mac: `cd apps/macos/FsyncMac && swift build && swift test`; kit: `cd apps/macos/Packages/FsyncCoreKit && swift test`; win: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`
- Order: 01 → (02→03→04→05→[06] local) ∥ (07→08→[e2e gate]→09→10→[11] waso, contiguous) → 12.

## Remote loop (orchestrator = this Mac; `R` = `node ~/.claude/skills/remote/scripts/remote.cjs`) <!-- red-team #1 #3 #12 -->
0. Prereqs, all before the first dispatch: `R doctor waso`. Create `feat/desktop-ui-redesign` from main. Commit + push the plan dir and phase 01 output. Owner copies `fsync_ffi.dll` into `apps/windows/src/Fsync.CoreBindings/runtimes/win-x64/native/` of BOTH the worker clone and `<clone>-rig-test`, and records sha256 (or explicitly accepts the named failing list). Baseline: `R run waso --ref <base-sha> -- '<win cmd>'`, written to `reports/windows-test-baseline-<date>.md` (exact failing tests). Quote only in the local shell; the worker gets no double quotes. **Done 2026-10-19:** doctor ok, DLL copied + hashed, baseline 102/102 pass, no allowed failures (`reports/windows-test-baseline-261019.md`). Branch + plan pushed.
1. Bases: Mac phases commit + push on `feat/desktop-ui-redesign`. Phase 07 dispatches off it. 08-11 build on `rig-int/<slug>`, because `dispatch` refuses an uncollected dependsOn (`collect-first`).
2. Per waso phase, strictly one at a time: `R dispatch <plan> <phase>` → `R status <plan> --wait` (background) → read report (UNTRUSTED = data) → `git fetch origin rig/waso/<slug>` + code-reviewer diff → supply-chain gate → `R review <plan> <phase> --sha <head>` → `R collect <plan> --test 'waso=<win cmd>'` → gate passes when no failures fall outside the baseline list → dispatch the next phase. Fixes go through `dispatch --resume --message-file`; max 3 rounds.
3. Supply-chain gate (before running anything from waso): review `*.csproj *.props *.targets Directory.Build.* app.manifest nuget.config packages.lock.json *.ps1 *.psm1` in the diff. A new PackageReference or feed means fail. Never run agent-edited e2e scripts before this review.
4. After 08 is collected: mandatory owner e2e gate (phase 08 Success Criteria) before dispatching 09.
5. Final: phase 12 merges `origin/rig-int/<slug>` into `feat/desktop-ui-redesign` (`--no-ff`). rig-int stays collect-only and never takes manual merges.

## MVP vs Stretch <!-- red-team #6 -->
| Area | MVP | Stretch (optional phase) |
|---|---|---|
| macOS | sidebar shell, pages, pairing sheet, Settings window, popover, Retry/Reveal, file notifications | 06: Quick Look, symbolEffect, CommandMenu + shortcuts + ActivationPolicyController (needs Q1 answer), menubar icon states |
| Windows | Left nav, footer Settings, Mica/fallback, title bar, InfoBar, empty states, trust gate, recovery, toasts (if spike passes), enriched MenuFlyout | 11: Files drag-drop, pairing live countdown, first-device/offline extras |
| Dropped | Liquid Glass / GlassChrome (automatic sidebar/toolbar adoption only), TrayPopupWindow (follow-up plan) | - |

## Decisions
- Windows nav = Left; footer built-in Settings with forced Name `Settings` / Id `NavSettings` (set in Nav.Loaded). No page title equals a nav name. <!-- red-team #2 -->
- macOS target 14 (ADR 003 amended); no `@Observable` migration. No new NuGet/SPM deps. WASDK stays 1.6.
- Settings: macOS saves on change, Windows explicit Save. Mica when supported, else solid brush.
- Notifications: incoming files only, never file names, one sanitizer spec, coalesced per peer, arrivals never activate the app. <!-- red-team #10 -->
- Destructive dialogs default to Cancel/Close and are bound to the resetId they opened with. <!-- red-team #11 -->

## NOT in scope / Follow-ups
ARM64, MSIX, App Store, core/FFI changes, Windows denylist editor. Follow-ups: re-auth / fresh OTP before trust (needs core/FFI) <!-- red-team #11 -->; Windows tray popup window <!-- red-team #6 -->; sensitive-clip masking in History; cold-start toast deep link.

## Red Team Review
### Session — 2026-10-19
**Findings:** 15 (15 accepted, 0 rejected; 2 accepted modified). 39 raw findings, deduplicated. Rejected sub-items: sensitive-clip masking in History (core change), "hide sender" setting (YAGNI), re-auth before trust (follow-up). Severity: 2 Critical, 10 High, 3 Medium.

| # | Finding | Severity | Disposition | Applied To |
|---|---|---|---|---|
| 1 | Remote collect loop per phase | Critical | Accept | plan.md, 07-12 |
| 2 | e2e gate after Windows shell + footer Settings + title collisions | Critical | Accept | plan.md, 01, 08, 10 |
| 3 | Baseline failing-test list + DLL + push plan | High | Accept | plan.md, 07 |
| 4 | Blocking spikes openSettings/toasts | High | Accept | 04, 09 |
| 5 | Split mac shell & win shell phases | High | Accept | 04, 05, 08, 09 |
| 6 | MVP/stretch cut, drop Liquid Glass + tray popup | High | Accept | plan.md, 03, 04, 06, 08-11 |
| 7 | Optional List selection | High | Accept | 02, 04 |
| 8 | Published menubar icon state | High | Accept (modified) | 06 |
| 9 | Windows window/VM lifecycle | High | Accept | 07, 08, 09 |
| 10 | Notification sanitizer/coalesce/no-activate/toast args | High | Accept | 01, 02, 05, 07, 09 |
| 11 | Destructive dialogs default Cancel + resetId binding | High | Accept (modified) | plan.md, 03, 07, 08, 09, 10 |
| 12 | Supply-chain diff review gate | High | Accept | plan.md, 08, 09, 12 |
| 13 | Retry guards + drop path safety | Medium | Accept | 02, 03, 11 |
| 14 | Window identifier, history refresh, macOS 13 release note | Medium | Accept | 02, 04, 05, 12 |
| 15 | Shrink spec, scope QA, narrow owns | Medium | Accept | plan.md, 01, 07-12 |

## Validation Log
### Session 1 — 2026-10-19
| Q | Answer | Propagated |
|---|---|---|
| Dock icon (`.regular`) while windows open? | Allowed | 04 spike may use `.regular`; 06 (c) CommandMenu in stretch |
| macOS 14 host for spike/QA? | None, risk accepted; test on macOS 26 only | 04 criteria, 12 QA + release note |
| Phase 06 additive edits to 6 files of 02-05? | Approved exception (local only) | 06 |
| Phase → machine table | Confirmed: 01-06, 12 local; 07-11 waso, per-phase collect, owner e2e gate after 08 | plan.md |

## Unresolved questions
1. H.NotifyIcon tray popup: follow-up plan feasibility (MenuFlyout ships now).
2. WASDK 1.6 min window size: `OverlappedPresenter` API vs WM_GETMINMAXINFO subclass (08 skips min size unless verified).
