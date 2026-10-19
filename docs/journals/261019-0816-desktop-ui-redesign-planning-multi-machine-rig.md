# Desktop UI Redesign Planning and Multi-Machine Rig Remote Loop

**Date**: 2026-10-19 08:16
**Severity**: Medium
**Component**: apps/macos (SwiftUI), apps/windows (WinUI 3), Rig remote execution (waso)
**Status**: Planned

## What Happened

Planned the macOS SwiftUI and Windows WinUI 3 desktop redesign on a shared IA (Devices, History, Files, Pair, Settings). Plan: `plans/261019-0016-desktop-ui-redesign-macos-windows/plan.md`, 12 phases, branch `feat/desktop-ui-redesign`. No code implemented. No core/FFI change.

Phase split: 01-06 and 12 local (Mac), 07-11 on waso via Rig. Windows phases run one at a time: `dispatch` → `status --wait` → read report → diff review → supply-chain gate → `review` → `collect` → next phase.

Remote setup: waso (Windows, cmd, user `dev`) has a Rig-dedicated clone at `C:\Users\dev\workspace\rig-fsync-native`. The owner's manual clone `C:\Users\dev\workspace\fsync-native` is never touched by Rig. laco-staging skipped for this project.

## Impact

Plan is the merge gate for both shells. Windows phases are blocked until waso has an interactive `dev` logon (see below). Remote loop is slower than local but keeps Windows changes reviewable per phase.

## Technical Details

- Red team: 4 lenses, 39 raw findings, 15 accepted (2 Critical, 10 High, 3 Medium).
  - Critical: per-phase collect loop (dispatch requires collect-first); no e2e/UIA check until the end → owner e2e gate after phase 08.
- Rig bug: child-process env allowlist dropped Windows system-folder vars (`ProgramFiles(x86)`, `ProgramData`, ...). NuGet restore failed with `Value cannot be null (path1)`. Fixed in Rig `b8738c5`, reinstalled on Mac and waso.
- Test blocker: Windows tests under headless `taskMode` (S4U logon) fail 60/102 with `CryptographicException: Access is denied` (S4U has no DPAPI keys). waso switched to `taskMode interactive`. This needs the `dev` user logged on to the waso desktop. It is not logged on now, so `doctor push-detached` hung in "starting" and the run was cancelled.
- Validation: Dock icon (`.regular`) allowed; macOS 14 untested, risk accepted; phase 06 additive-handoff ownership exception approved; machine table confirmed.

## What We Tried

- `set` prefix workarounds in cmd-shell worker commands. Rejected as fragile: cmd commands cannot contain double quotes. Fixed the allowlist instead.
- Headless scheduled-task test runs. Rejected for now: S4U logon breaks DPAPI.
- Dropped Liquid Glass and the Windows tray popup window from MVP (follow-up plan).

## Root Cause Analysis

- Env allowlist was written for POSIX-style minimal env. It missed Windows system vars that NuGet and MSBuild need. We assumed the child env matched the interactive shell.
- Headless `taskMode` was chosen as the default test path without checking DPAPI availability under S4U. Windows tests that touch credential storage need an interactive logon.

## Key Decisions

- Windows nav = Left; footer Settings. macOS target 14 (ADR 003 amended). No new NuGet/SPM deps. WASDK stays 1.6.
- Per-phase collect; owner e2e gate after phase 08 before 09.
- Supply-chain gate: any new PackageReference or feed in diff fails.
- Trade-off: slower serial Windows loop, accepted for reviewability.

## Lessons Learned

- Fix the tool's env allowlist rather than working around it per command.
- Verify the execution context (logon type, DPAPI, interactive session) before writing a test plan that depends on it.
- Keep remote-worker commands free of double quotes on cmd.

## Next Steps

- Owner: log `dev` on to waso desktop, then `R doctor waso` (blocks all Windows phases). Before phase 07 dispatch.
- Owner: copy `fsync_ffi.dll` into `apps/windows/src/Fsync.CoreBindings/runtimes/win-x64/native/` (worker clone and `<clone>-rig-test`), record sha256. Before phase 07.
- Capture Windows baseline failing list into `reports/windows-test-baseline-<date>.md`. Before phase 07.
- Create `feat/desktop-ui-redesign` from main; commit and push plan dir. Before phase 01 starts.

## Unresolved Questions

1. H.NotifyIcon tray popup: follow-up plan feasibility (MenuFlyout ships now).
2. WASDK 1.6 min window size: `OverlappedPresenter` vs `WM_GETMINMAXINFO` subclass (phase 08 skips min size unless verified).
3. macOS 14 never tested (risk accepted); QA runs on macOS 26 only. Red team #14 adds a macOS 13 release note; confirm wording before release.
