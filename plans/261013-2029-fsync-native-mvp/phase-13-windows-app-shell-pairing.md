# Phase 13 — Windows app: tray shell, login, pairing, device list

## Context Links
- Design §2, §4 flows 1-2; [phase-03](phase-03-spike-ffi-bindings.md) ADR 008; [phase-05](phase-05-spike-windows-clipboard-lazy-paste.md) ADR 010; [phase-09](phase-09-backend-auth-registry-presence.md)

## Overview
- Priority: P1 | Status: pending | Effort: 4-5 wk (D) — parallel with phase 11
- WinUI 3 (Windows App SDK, unpackaged + Velopack) tray app with H.NotifyIcon, bindings project (uniffi-bindgen-cs or C ABI per ADR 008), login, pairing, device list, start with Windows.

## Key Insights
- Unpackaged WinUI 3 (self-contained WinAppSDK) simplifies Velopack updates; MSIX not needed.
- Callbacks from Rust threads → `DispatcherQueue.TryEnqueue`; single `EngineBridge` (CommunityToolkit.Mvvm ObservableObject).
- Secrets: device key + refresh token + history key (32 B, generated on first launch) in DPAPI-protected file <!-- Updated: Validation Session 1 - history_key --> — (`ProtectedData`, CurrentUser) or Credential Locker; pick DPAPI (size limits of Credential Locker).

## Requirements
- Tray icon w/ status badge, left-click flyout (devices + recent), right-click menu; Settings/onboarding windows created on demand (memory).
- Start at login: `HKCU\...\Run` entry (unpackaged).
- x64 + ARM64 builds.

## Architecture
```
apps/windows/
  Fsync.sln
  src/Fsync.App/            App.xaml(.cs), TrayHost.cs, Views/{Onboarding,Pairing,Devices,Settings}/*.xaml(.cs), ViewModels/*.cs
  src/Fsync.App/Bridge/     EngineBridge.cs, EngineDelegateAdapter.cs, SecretStore.cs (DPAPI)
  src/Fsync.CoreBindings/   generated C# (or csbindgen + wrapper) + native DLL per RID
  src/Fsync.Interop/        Win32/COM (CsWin32 NativeMethods.txt) — used heavily in phase 14
  tests/Fsync.App.Tests/    xUnit for view models/bridge mapping
```
Data flow: View → ViewModel → EngineBridge → FsyncEngine (FFI) → core; events → adapter → DispatcherQueue → ViewModel → View.

## Related Code Files
- Create: above. Modify: none outside `apps/windows/**`.

## Implementation Steps
1. Solution, unpackaged WinUI 3, TrayHost (H.NotifyIcon), single-instance (AppInstance redirect).
2. CoreBindings project consuming CI DLL artifacts per RID.
3. EngineBridge + SecretStore; engine start on launch.
4. Login/OTP; pairing offer (QR via QRCoder) + join + SAS confirm.
5. Device list w/ status + remove.
6. Run-at-login toggle; settings shell.
7. xUnit tests for VM mapping.

## Todo List
- [ ] Solution + tray + single instance
- [ ] CoreBindings
- [ ] EngineBridge + DPAPI store
- [ ] Login/OTP
- [ ] Pairing offer/join/SAS
- [ ] Devices + remove
- [ ] Run at login
- [ ] Tests

## Success Criteria
- Win pairs with Mac (phase 11) via code + SAS; status LAN/relay/offline correct within 5 s; ARM64 build runs.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| H.NotifyIcon/WinUI 3 quirks | M x M | WPF tray host fallback (ADR 010) |
| Native DLL load path per arch | M x L | RID-specific runtime folders, startup self-check |
| Windows Firewall prompt for UDP | M x M | installer adds rule (phase 16); onboarding explains |

## Rollback
- App-only; revert commits.

## Security Considerations
- DPAPI CurrentUser scope; SAS confirm mandatory; no admin rights required.

## Next Steps
- Phase 14 adds clipboard/files/history.
