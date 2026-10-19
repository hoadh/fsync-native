---
machine: waso
owns: ["apps/windows/src/Fsync.App/App.Parity.cs", "apps/windows/src/Fsync.App/MainWindow.TrustGate.cs", "apps/windows/src/Fsync.App/Notifications/ToastNotifier.cs", "apps/windows/src/Fsync.App/Views/TrustResetPage.xaml", "apps/windows/src/Fsync.App/Views/TrustResetPage.xaml.cs", "apps/windows/src/Fsync.App/TrayHost.cs", "apps/windows/src/Fsync.App/Program.cs"]
dependsOn: [08]
---

# Phase 09: Windows trust gate, toasts, tray menu (waso)
<!-- red-team #5: split from old Windows shell phase -->
<!-- red-team #12: owns exact; no csproj, no app.manifest -->

## Context Links
- Spec: §Notification rules, §Sanitizer, §Destructive dialog rule
- Phase 07: `TrustResetPromptViewModel`, `AccountRecoveryViewModel`, `IncomingFilesNotice(+Coalescer)`, `ToastAction`, `TransfersViewModel.OfferReceived`
- Phase 08: seams `OnShowPhaseGate`, `OnWindowCreatedExtra`, `OnWindowClosedExtra`, `OnEngineReady`, `OnQuitting`; `App.VisiblePage`, `App.IsWindowActive`; `Dialogs.ConfirmAsync`
- Current: `TrayHost.cs` (H.NotifyIcon), `Program.cs` (single instance, entry)

## Overview
- Priority: P1. Status: pending.
- Wire the parity VMs into the app: trust-reset gate page, unpackaged toasts for incoming files, and an enriched tray MenuFlyout. Task 1 is a blocking toast spike.
- TrayPopupWindow dropped (follow-up). <!-- red-team #6 -->

## Key Insights
- **Spike (task 1)** <!-- red-team #4 -->: unpackaged `AppNotificationManager.Default.Register()` must run in `Program.Main` before the app starts, and `Unregister()` on quit. The worker implements the minimal path; after diff review + collect, the owner verifies on Win10 AND Win11 (interactive): a toast appears, a click with the app running opens the Files page, a click with the app closed starts it and just opens the main window. Fail on either OS → `ToastNotifier` becomes a no-op behind `IsSupported`, the tray badge (tooltip count) is the only signal, and the result is recorded in `reports/windows-toast-spike-<date>.md`.
- Toast args are constant: only `action=open-files`; no sender, file names or ids in args/tag. Body text from `IncomingFilesNotice` (sanitized). <!-- red-team #10 -->
- `NotificationInvoked` arrives on a background thread → `DispatcherQueue.TryEnqueue`. `ToastAction.TryParse` false → ignore. Cold start (launch from a toast): only open the main window, no navigation side effects. <!-- red-team #10 -->
- No activation on arrival: posting a toast never shows or activates the window. Suppressed when `App.IsWindowActive && App.VisiblePage == typeof(TransfersPage)`.
- Trust gate via `OnShowPhaseGate`: `Ready` + `TrustReset.HasPending` → `gated = true`, frame shows `TrustResetPage`. Re-run in the MainWindow ctor (`OnWindowCreatedExtra`) and in `App.ShowWindow`, so a window recreated from the tray is gated too. `HasPending` change → re-run `ShowPhase`. <!-- red-team #9 -->
- TrustResetPage: device names from `DisplayNames` (sanitized); confirm uses `BeginConfirm(id)` → `Dialogs.ConfirmAsync` (Close default) → `Confirm(id)`; `ConfirmAborted` → `dialog.Hide()` + notice. <!-- red-team #11 -->
- `App.Parity.cs` (partial App): creates `TrustReset` and `Recovery` VMs in `OnEngineReady`, wires `OfferReceived` → coalescer → toast, disposes in `OnQuitting`. Exposes `App.TrustReset`, `App.Recovery` (phase 10 uses them).
- Tray: enrich the existing H.NotifyIcon `MenuFlyout`: Open fsync, Files (n) when offers exist, Pair a device…, Settings, Quit. Tooltip shows pending offer count.

## Requirements
- `Program.cs`: register/unregister AppNotificationManager behind try/catch; log failure, never crash.
- `Notifications/ToastNotifier.cs`: `IsSupported`, `Show(IncomingFilesNotice)` (AppNotificationBuilder, constant args), `Invoked` event marshalled to the UI queue.
- `App.Parity.cs`: VM creation/dispose, toast wiring, suppression, `ToastAction.OpenFiles` → `ShowWindow` + navigate to Files (TransfersPage).
- `MainWindow.TrustGate.cs`: implements the 3 MainWindow seams; subscribes to `TrustReset.PropertyChanged(HasPending)` and unsubscribes in `OnWindowClosedExtra`.
- `Views/TrustResetPage.xaml(.cs)`: list of sanitized names, Keep current devices (default) / Confirm reset (destructive), outcome InfoBar.
- `TrayHost.cs`: flyout items + tooltip count.

## Architecture
```
Program.Main → AppNotificationManager.Register → App
App.OnEngineReady → TrustReset, Recovery VMs; Transfers.OfferReceived → Coalescer → Notice
   → suppressed? drop : ToastNotifier.Show(constant args)
toast click → Invoked (bg) → DispatcherQueue → TryParse → ShowWindow → Files page
MainWindow.ShowPhase → OnShowPhaseGate → HasPending ? TrustResetPage : route
```

## Related Code Files
- Modify: `Program.cs`, `TrayHost.cs`
- Create: `App.Parity.cs`, `MainWindow.TrustGate.cs`, `Notifications/ToastNotifier.cs`, `Views/TrustResetPage.xaml(.cs)`
- Delete: none
(paths relative to `apps/windows/src/Fsync.App/`)

## Implementation Steps
1. Toast spike path: Program register/unregister + ToastNotifier minimal + one hard-coded notice via debug flag removed before commit; report section for the owner.
2. App.Parity: VMs, OfferReceived wiring, suppression, invoked routing.
3. Trust gate partial + TrustResetPage + dialog binding.
4. Tray flyout + tooltip count.
5. Phase test command.

## Todo List
- [ ] Toast spike (owner verifies Win10 + Win11)
- [ ] App.Parity wiring + suppression + routing
- [ ] Trust gate + TrustResetPage + confirm binding
- [ ] Tray MenuFlyout + tooltip
- [ ] Spike report

## Success Criteria
- waso: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`, no failures outside the baseline list.
- `git diff --stat` touches only `owns`; no csproj/manifest/package change.
- `reports/windows-toast-spike-<date>.md` records PASS/FAIL on Win10 and Win11 and the chosen behavior.
- Owner manual: a trust reset from `fsync-cli` gates the window (also after close + reopen from tray); Return = Keep current devices; 3 offers in 10 s → one toast; no focus steal on arrival.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Unpackaged toasts fail on an OS | M x M | Spike; no-op fallback + tray count |
| Toast callback off UI thread | H x H if unhandled | DispatcherQueue marshal |
| Gate missed on recreated window | M x H | Gate in ctor + ShowWindow |
| Malicious toast args | L x M | Constant args; TryParse allowlist |

## Security Considerations
- Toast payload has no file names, paths or ids. Peer names sanitized. Trust confirm only on the dialog's resetId, Cancel default.

## Next Steps
- Phase 10 uses `App.TrustReset`, `App.Recovery`, `BannerHost`. Rollback: revert the collected merge on rig-int; the shell from 08 keeps working without gate/toasts.
