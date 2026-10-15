# Phase 13 — Windows app: tray shell, login, pairing, device list

## Context Links
- Design §2, §4 flows 1-2; [phase-03](phase-03-spike-ffi-bindings.md) ADR 008; [phase-05](phase-05-spike-windows-clipboard-lazy-paste.md) ADR 010; [phase-09](phase-09-backend-auth-registry-presence.md)

## Overview
- Priority: P1 | Status: Partial (code, unit tests, e2e vs fsync-cli pass; Mac↔Win code+SAS pairing PASS via Track X over Tailscale 2026-10-14; S5 relay list + DPAPI pending revoke merged 7cc8cfd 2026-10-15; open: Win GUI manual run, Mac↔Win LAN path (S7 blocked), ARM64 hardware) | Effort: 4-5 wk (D) — parallel with phase 11
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
- [x] Solution + tray + single instance (AppInstance per data dir; second launch redirects, verified)
- [x] CoreBindings (generated C# + runtimes/<rid>/native, NativeLoader; startup contract self-check)
- [x] EngineBridge + DPAPI store
- [x] Login/OTP (e2e: app signs in with the OTP over UI Automation)
- [x] Pairing offer (code + QR)/join/SAS (unit-tested + e2e)
- [x] Devices + remove (unit-tested + e2e)
- [x] Run at login (HKCU Run, --background)
- [x] Tests (dotnet 144/144 on merged main 2026-10-15: CoreBindings 9, Interop 47, App 88; tests/e2e/pair-with-cli.ps1 PASS)
- [x] S5: regenerated C# bindings, `FSYNC_RELAY` comma list, DPAPI pending revoke (7cc8cfd, review 8.8→9.5)
- Round 2 (B1, 2026-10-15, merged local main 3c405b1): S5 lows fixed. Pending revoke is set in memory only after a successful Save; `WithoutAnEngineSignOutRevokesDirectly` flake fixed (15x green).
- [ ] Win app GUI pairing run with Mac (Track X did not run the GUI)
- [ ] ARM64 hardware run

Build note: `fsync_ffi.dll` is gitignored. Rebuild it together with the bindings before running dotnet tests.

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
