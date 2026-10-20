---
machine: waso
owns: ["apps/windows/src/Fsync.App/App.xaml", "apps/windows/src/Fsync.App/App.xaml.cs", "apps/windows/src/Fsync.App/MainWindow.xaml", "apps/windows/src/Fsync.App/MainWindow.xaml.cs", "apps/windows/src/Fsync.App/Views/Dialogs.cs", "apps/windows/src/Fsync.App/Views/Ui.cs", "apps/windows/src/Fsync.App/Styles/Tokens.xaml", "apps/windows/src/Fsync.App/Shell/TitleBarSetup.cs", "apps/windows/src/Fsync.App/Shell/BackdropSetup.cs", "apps/windows/src/Fsync.App/Shell/NavigationRoutes.cs", "apps/windows/tests/e2e/Uia.psm1", "apps/windows/tests/e2e/pair-with-cli.ps1", "apps/windows/tests/e2e/clipboard-with-cli.ps1"]
dependsOn: [07]
status: in-progress
---

# Phase 08: Windows shell: Mica, title bar, left nav, e2e helpers (waso)
<!-- red-team #5: shell split; trust gate, toasts, tray → phase 09 -->
<!-- red-team #12: no csproj, no app.manifest in owns; csproj needs no Page items -->
<!-- red-team #15: owns narrowed to exact files -->

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` (Windows titles, AutomationIds, tokens)
- Research: `research/researcher-261019-winui3-fluent-patterns.md` (Mica, ExtendsContentIntoTitleBar, NavigationView footer Settings)
- Current: `apps/windows/src/Fsync.App/MainWindow.xaml` (`PaneDisplayMode="Top"`, `IsSettingsVisible="False"`), `MainWindow.xaml.cs`, `App.xaml.cs` (`PendingSend`), `Views/Dialogs.cs` (`ShowAsync`), `Views/Ui.cs`, `tests/e2e/Uia.psm1` (Find-Name, Invoke-Name, Get-Names)
- Phase 07 VMs (not wired yet: phase 09)

## Overview
- Priority: P1. Status: pending.
- Left NavigationView with footer Settings, Mica backdrop, custom title bar, design tokens, window/VM lifecycle, `Dialogs.ConfirmAsync`, and e2e helper updates for the renamed nav items. Ends with a mandatory owner e2e gate.

## Key Insights
- Renames <!-- red-team #2 -->: nav item names stay "Devices", "History", "Files", "Pair a device", "Settings" (e2e-locked, spec). Page titles change (phase 10) to "Your devices", "Clipboard history", "Files and transfers", "Pair this PC", "Preferences", so `Find-Name` on a nav item never matches a page header.
- Footer Settings <!-- red-team #2 -->: `IsSettingsVisible="True"`; the built-in SettingsItem exists only after the template is applied. In `Nav.Loaded`: `if (Nav.SettingsItem is NavigationViewItem s) { AutomationProperties.SetName(s, "Settings"); AutomationProperties.SetAutomationId(s, "NavSettings"); s.Tag = NavigationRoutes.SettingsTag; }`. `Select(SettingsTag)` before Loaded is queued and replayed in Loaded.
- Tag routing: `NavigationRoutes` maps tag → page type; `ItemInvoked` with `IsSettingsInvoked` maps to SettingsPage.
- Window/VM lifecycle <!-- red-team #9 -->: VMs live on `App` (one instance, created once after engine Ready); MainWindow is recreated on reopen from tray. MainWindow subscribes in the ctor and unsubscribes in `Closed`. App exposes `VisiblePage` (Type?) and `IsWindowActive` (Activated/Closed) for toast suppression (phase 09).
- Partial-method seams so phase 09 never edits these files <!-- red-team #5 -->:
  - `MainWindow`: `partial void OnShowPhaseGate(AppPhase phase, ref bool gated);`, `partial void OnWindowCreatedExtra();`, `partial void OnWindowClosedExtra();`
  - `App`: `partial void OnEngineReady();`, `partial void OnQuitting();`
  - Unimplemented partial methods compile to nothing.
- `Dialogs.ConfirmAsync(XamlRoot, title, body, primaryText)` is additive: `DefaultButton = ContentDialogButton.Close`, Close "Cancel". Existing `ShowAsync` unchanged. <!-- red-team #11 -->
- Mica: `MicaBackdrop` when `MicaController.IsSupported()`, else `DesktopAcrylicBackdrop`, else none (Win10 → solid). Title bar: `ExtendsContentIntoTitleBar = true` + `SetTitleBar(AppTitleBar)`; caption buttons stay system. Min size via `AppWindow.Presenter` (`OverlappedPresenter.PreferredMinimumWidth/Height` if WASDK 1.6 has it; else a WM_GETMINMAXINFO subclass, plan Q4).
- csproj already globs XAML (no explicit Page items); `Styles/Tokens.xaml` merges via `App.xaml`. No csproj/app.manifest/package edit. <!-- red-team #12 -->

## Requirements
- `MainWindow.xaml`: `PaneDisplayMode="Left"`, `IsSettingsVisible="True"`, `IsBackButtonVisible="Collapsed"`; menu items Devices / History / Files / Pair a device (Content unchanged) with `AutomationProperties.AutomationId` per spec (`NavDevices`, `NavHistory`, `NavFiles`, `NavPairing`); InfoBar host `BannerHost` (named, for phase 09/10); AppTitleBar grid.
- `MainWindow.xaml.cs`: route via `NavigationRoutes`; `ShowPhase` calls `OnShowPhaseGate` before choosing the page; lifecycle hooks; footer Settings setup.
- `App.xaml(.cs)`: merge Tokens; `VisiblePage`, `IsWindowActive`; seams; `PendingSend` unchanged.
- `Styles/Tokens.xaml`: spacing (4/8/12/16/24), corner radius, type ramp aliases, status brushes (Online, Lan, Relay, Offline, Attention). Keys documented in the file header (consumed by phase 10).
- `Ui.cs`: `StatusBrush(PeerStatus)`, `PageHeader(title, subtitle)` helpers.
- e2e: `Uia.psm1` adds `Find-AutomationId`/`Invoke-AutomationId`; scripts navigate by AutomationId (`NavDevices`, `NavPairing`, `NavSettings`) and stop depending on page title text.

## Architecture
```
App (VMs, bridge, PendingSend, VisiblePage, IsWindowActive)
 └ MainWindow (recreated) ── TitleBar | NavigationView(Left, footer Settings) ── Frame
      ShowPhase(phase): OnShowPhaseGate(phase, ref gated) [09] → gated ? (09 page) : NavigationRoutes(tag)
      Closed → unsubscribe + OnWindowClosedExtra
```

## Related Code Files
- Modify: `App.xaml`, `App.xaml.cs`, `MainWindow.xaml`, `MainWindow.xaml.cs`, `Views/Dialogs.cs`, `Views/Ui.cs`, `tests/e2e/{Uia.psm1,pair-with-cli.ps1,clipboard-with-cli.ps1}`
- Create: `Styles/Tokens.xaml`, `Shell/{TitleBarSetup,BackdropSetup,NavigationRoutes}.cs`
- Delete: none
(app paths relative to `apps/windows/src/Fsync.App/`)

## Implementation Steps
1. Tokens + App merge; NavigationRoutes.
2. MainWindow left nav + footer Settings (Loaded guard + queued select) + BannerHost + title bar + backdrop.
3. Lifecycle: subscribe in ctor, unsubscribe on Closed; App `VisiblePage`/`IsWindowActive`; seams declared.
4. `Dialogs.ConfirmAsync`; `Ui` helpers.
5. e2e helpers + scripts by AutomationId.
6. Phase test command on waso.

## Todo List
- [ ] Tokens + routes
- [ ] Left nav + footer Settings + title bar + Mica
- [ ] Lifecycle + seams + App members
- [ ] Dialogs.ConfirmAsync (Close default) + Ui helpers
- [ ] e2e helpers by AutomationId
- [ ] Owner e2e gate report

## Success Criteria
- waso: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`, no failures outside the baseline list.
- Members later phases use exist with these exact names (grep in the diff): `Dialogs.ConfirmAsync`, `App.PendingSend`, `App.VisiblePage`, `App.IsWindowActive`, `OnShowPhaseGate`, `OnWindowCreatedExtra`, `OnWindowClosedExtra`, `OnEngineReady`, `OnQuitting`, `BannerHost`, Tokens keys listed in the file header. <!-- red-team #15 -->
- `git diff --stat` shows no csproj, props, app.manifest or package change.
- **Mandatory owner e2e gate** <!-- red-team #2 -->: after review + `collect --test`, at the `rig-int/<slug>` sha, the owner on the waso interactive desktop runs the app, dumps the UIA tree with `Get-Names`, and runs `pair-with-cli.ps1` and `clipboard-with-cli.ps1`. Pass = NavDevices/NavHistory/NavFiles/NavPairing/NavSettings found, Names Devices/History/Files/Pair a device/Settings, no page title equals a nav item name, both scripts pass. Result in `reports/windows-e2e-gate-<date>.md`. Phase 09 is not dispatched before PASS.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| SettingsItem null before template | H x M | Loaded guard + queued select |
| e2e breaks on renames | M x H | AutomationId helpers + owner gate before 09 |
| Event leak on window recreate | M x M | Unsubscribe on Closed; reopen test in gate (open/close from tray 3x) |
| Mica unsupported (Win10) | M x L | Fallback chain to solid |
| Min size API missing in 1.6 | M x L | Subclass fallback; plan Q4 |

## Security Considerations
- No new inputs. Destructive confirms default to Cancel.

## Next Steps
- Remote loop with owner gate, then dispatch 09. Rollback: revert the collected merge on rig-int; phase 07 VMs stay unused.
