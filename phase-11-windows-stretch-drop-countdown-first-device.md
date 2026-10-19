---
machine: waso
owns: ["apps/windows/src/Fsync.App/Views/TransfersPage.Drop.cs", "apps/windows/src/Fsync.App/Views/PairingPage.Countdown.cs", "apps/windows/src/Fsync.App/Views/DevicesPage.Extras.cs", "apps/windows/src/Fsync.App.Core/Files/DropPaths.cs", "apps/windows/src/Fsync.App.Core/Bridge/EngineBridge.FirstDevice.cs", "apps/windows/src/Fsync.App.Core/ViewModels/PairingViewModel.cs", "apps/windows/tests/Fsync.App.Tests/DropPathsTests.cs", "apps/windows/tests/Fsync.App.Tests/PairingCountdownTests.cs", "apps/windows/tests/Fsync.App.Tests/FirstDeviceTests.cs"]
dependsOn: [10]
---

# Phase 11 (OPTIONAL stretch): Windows file drop, pairing countdown, first-device banner (waso)
<!-- red-team #6: stretch items moved out of MVP phases 07/10 -->

## Context Links
- Phase 10 seams: `SetupFileDrop(DropHost)`, `SetupCountdown(CountdownText)`, `AddExtraBanners(ExtraBannerHost)`
- Phase 08: `Dialogs.ConfirmAsync`; `App.PendingSend` (existing send path)
- macOS reference: `DropSendRequest.validate` (phase 02)

## Overview
- Priority: P3. Status: pending. Optional; each item independently droppable. Run only after phase 10 is collected.
- Items: (a) drag files onto Transfers → confirm → send; (b) pairing code countdown; (c) "This is your first device" banner on Devices.

## Key Insights
- Partial implementations only; no edit to phase 08/10 files. `FakeEngine` not edited: tests use the existing fake surface; if something is missing, drop the item and report. <!-- red-team #15 -->
- Drop safety <!-- red-team #13 -->: `DropPaths.Validate(IEnumerable<string>)`:
  - accept only local file paths (`Path.IsPathFullyQualified`, not UNC `\\`, not device paths `\\?\`/`\\.\`);
  - resolve symlinks/junctions (`FileSystemInfo.ResolveLinkTarget(true)`), reject if the target is a directory or missing;
  - de-duplicate, cap count per spec;
  - returns accepted + rejected-with-reason.
  - The page shows the full accepted list (name + size) in `ConfirmAsync` (Close default) before `App.PendingSend`.
- Countdown: `PairingViewModel` exposes `RemainingSeconds` from the existing expiry, ticks via `DispatcherQueueTimer` in the page partial; stops on Closed.
- First device: `EngineBridge.FirstDevice.cs` adds `IsFirstDevice` (no trusted peers + Ready).

## Requirements
- (a) `TransfersPage.Drop.cs`, `DropPaths.cs` + tests (UNC, device path, symlink to dir, missing, dup).
- (b) `PairingPage.Countdown.cs`, `PairingViewModel.RemainingSeconds` + tests (fake TimeProvider).
- (c) `DevicesPage.Extras.cs`, `EngineBridge.FirstDevice.cs` + tests.
- Phase report lists shipped vs dropped items.

## Architecture
```
DragOver/Drop(DropHost) → StorageItems → paths → DropPaths.Validate → ConfirmAsync(full list) → App.PendingSend
PairingViewModel.RemainingSeconds ← expiry; page timer → CountdownText
EngineBridge.IsFirstDevice → DevicesPage.AddExtraBanners → InfoBar
```

## Related Code Files
- Create: all frontmatter files except `PairingViewModel.cs`
- Modify: `apps/windows/src/Fsync.App.Core/ViewModels/PairingViewModel.cs` (additive property)
- Delete: none

## Implementation Steps
1. DropPaths + tests; Drop partial.
2. Countdown property + tests; page partial.
3. FirstDevice partial + tests; banner partial.
4. Phase test command.

## Todo List
- [ ] (a) DropPaths + drop partial
- [ ] (b) Countdown
- [ ] (c) First-device banner
- [ ] Report shipped vs dropped

## Success Criteria
- waso: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`, no failures outside the baseline list; new tests pass for shipped items.
- `git diff --stat` touches only `owns`.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Drop sends unintended files | L x H | Validate + full-list confirm, Cancel default |
| Timer leak | L x L | Stop on Unloaded/Closed |
| PairingViewModel conflict | L x M | Only phase owning it; additive |

## Security Considerations
- Drop rejects UNC/device paths and resolves links; nothing is sent without explicit confirmation.

## Next Steps
- Phase 12 QA rows for shipped items. Rollback: revert the collected merge; MVP unaffected (seams become no-ops).
