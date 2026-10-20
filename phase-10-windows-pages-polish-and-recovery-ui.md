---
machine: waso
owns: ["apps/windows/src/Fsync.App/Views/DevicesPage.xaml", "apps/windows/src/Fsync.App/Views/DevicesPage.xaml.cs", "apps/windows/src/Fsync.App/Views/HistoryPage.xaml", "apps/windows/src/Fsync.App/Views/HistoryPage.xaml.cs", "apps/windows/src/Fsync.App/Views/OnboardingPage.xaml", "apps/windows/src/Fsync.App/Views/OnboardingPage.xaml.cs", "apps/windows/src/Fsync.App/Views/PairingPage.xaml", "apps/windows/src/Fsync.App/Views/PairingPage.xaml.cs", "apps/windows/src/Fsync.App/Views/SettingsPage.xaml", "apps/windows/src/Fsync.App/Views/SettingsPage.xaml.cs", "apps/windows/src/Fsync.App/Views/TransfersPage.xaml", "apps/windows/src/Fsync.App/Views/TransfersPage.xaml.cs", "apps/windows/src/Fsync.App/Views/Controls/EmptyState.xaml", "apps/windows/src/Fsync.App/Views/Controls/EmptyState.xaml.cs", "apps/windows/src/Fsync.App/Views/Controls/AccountRecoveryFlow.cs", "apps/windows/src/Fsync.App/Views/Controls/PageText.cs", "apps/windows/src/Fsync.App.Core/ViewModels/HistoryViewModel.cs", "apps/windows/src/Fsync.App.Core/Files/TransferItem.cs", "apps/windows/src/Fsync.App.Core/Clipboard/ClipboardService.Incoming.cs", "apps/windows/src/Fsync.App.Core/Clipboard/ClipboardService.Downloads.cs", "apps/windows/tests/Fsync.App.Tests/PeerNameDisplayTests.cs"]
dependsOn: [09]
status: in-progress
---

# Phase 10: Windows pages polish and recovery UI (waso)
<!-- red-team #15: owns narrowed to exact files; e2e scripts stay with phase 08 -->

## Context Links
- Spec: copy table (Windows titles), icon map, state matrix, destructive dialog rule
- Phase 08: Tokens keys, `Ui.PageHeader`, `Ui.StatusBrush`, `Dialogs.ConfirmAsync`, `BannerHost`; phase 09: `App.TrustReset`, `App.Recovery`
- Current pages: `apps/windows/src/Fsync.App/Views/*Page.xaml(.cs)`

## Overview
- Priority: P1. Status: pending.
- Restyle the 6 pages with tokens + headers + empty states, rename page titles, add the account recovery UI (Settings → Account), and declare partial seams for stretch phase 11.

## Key Insights
- Titles <!-- red-team #2 -->: "Your devices", "Clipboard history", "Files and transfers", "Pair this PC", "Preferences" (Onboarding unchanged). Nav item names never appear as page headers.
- No e2e script edits here. If a script must change to pass, report BLOCKED (the phase 08 gate already covers renames).
- `DeviceItem.ToString()` (UIA name of rows) untouched; row templates may add children but keep the list item Name.
- Recovery <!-- red-team #11 -->: `AccountRecoveryFlow` drives `App.Recovery` Explain → FinalConfirm via two `Dialogs.ConfirmAsync` (Close default, Cancel). Device names in body via `PeerText.Sanitize`. `ReauthRequired` → sign-in screen; a richer re-auth flow is a Follow-up (plan.md).
- Seams for phase 11 (partial classes; these files declare, 11 implements) <!-- red-team #6 -->:
  - `TransfersPage`: `partial void SetupFileDrop(UIElement host);` + named `DropHost`
  - `PairingPage`: `partial void SetupCountdown(TextBlock host);` + named `CountdownText` (Collapsed by default)
  - `DevicesPage`: `partial void AddExtraBanners(StackPanel host);` + named `ExtraBannerHost`
- Pages must be `partial class` (already true for XAML code-behind).

## Requirements
- Each page: `Ui.PageHeader`, token spacing, `EmptyState` control (glyph, title, body, optional action) for empty lists.
- Devices: status via `Ui.StatusBrush`, platform glyphs from spec, Remove… via `ConfirmAsync`.
- History: search box, empty/no-results states.
- Transfers: offers section (sanitized sender), list with Cancel/Retry/Open folder, Clear finished.
- Settings: grouped sections (General, Sync, Privacy, Storage, Account); Account has Sign out (confirm) and "Reset account…" → `AccountRecoveryFlow`.
- Onboarding + Pairing: spacing/tokens only; behavior unchanged.

## Architecture
```
Page ── PageHeader(title) ── content | EmptyState
SettingsPage.Account ── AccountRecoveryFlow(App.Recovery, XamlRoot) ── ConfirmAsync x2 ── ResetAsync
Seams: SetupFileDrop / SetupCountdown / AddExtraBanners (no-ops until phase 11)
```

## Related Code Files
- Modify: the 6 pages (xaml + xaml.cs)
- Create: `Views/Controls/EmptyState.xaml(.cs)`, `Views/Controls/AccountRecoveryFlow.cs`
- Delete: none

## Implementation Steps
1. EmptyState control.
2. Pages restyle + titles; seams + named hosts.
3. AccountRecoveryFlow + Settings Account section.
4. Phase test command; owner re-runs the phase 08 e2e scripts at the collected sha.

## Todo List
- [ ] EmptyState
- [ ] 6 pages restyled + renamed titles
- [ ] Seams + named hosts
- [ ] Recovery UI (Close default, sanitized names)
- [ ] e2e re-run (owner)
- [ ] Carried from 08 review: DevicesPage dot binds `Style="{x:Bind local:Ui.StatusDotStyle(Kind)}"` (theme-aware), then delete `Ui.StatusBrush(StatusKind)`; toggle Light/Dark on Devices and confirm dots recolour
- [ ] Carried from 09 review (owner-approved owns exception: `Fsync.App.Core/ViewModels/TrustResetPromptViewModel.cs` + its tests): clear `Pending`/`Outcome` on SignedOut so a failed first check after an account switch cannot show the old account's prompt; comment on the `RemovedNeedsPairing → Open` test

## Success Criteria
- waso: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`, no failures outside the baseline list.
- `git diff --stat` touches only `owns`; no `tests/e2e` change.
- Owner: `pair-with-cli.ps1` + `clipboard-with-cli.ps1` pass at the collected sha; `Get-Names` shows the 5 new page titles and no title equals a nav item.
- Every ContentDialog added here has `DefaultButton = Close`.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Restyle breaks UIA names | M x H | Keep `DeviceItem.ToString()`; owner e2e re-run |
| Accidental account reset | L x H | Two confirms, Close default |
| Seams unused → dead code | L x L | Partial methods compile away |

## Security Considerations
- Recovery is destructive: double confirm, sanitized device names, no auto-proceed.

## Next Steps
- Optional phase 11; else phase 12. Rollback: revert the collected merge; phases 08-09 unaffected.
