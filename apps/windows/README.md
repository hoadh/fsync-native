# fsync for Windows

Unpackaged, self-contained WinUI 3 tray app (Windows 10 22H2+ / 11, x64 + ARM64). Phase 13:
tray shell, email sign-in, pairing, device list, start with Windows. Clipboard and files: phase 14.

| Project | What |
|---|---|
| `src/Fsync.CoreBindings` | C# bindings for `fsync_ffi.dll` (uniffi-bindgen-cs, ADR 008) + native DLL per RID |
| `src/Fsync.App.Core` | No WinUI: `EngineBridge`, DPAPI `SecretStore`, view models, `RunAtLogin` |
| `src/Fsync.App` | WinUI 3 shell: `Program` (single instance), `App`, `TrayHost`, `MainWindow`, `Views/` |
| `tests/Fsync.CoreBindings.Tests` | contract tests against the real DLL |
| `tests/Fsync.App.Tests` | bridge / view model tests against a fake `IFsyncEngine` |
| `tests/e2e/pair-with-cli.ps1` | UI Automation: app signs in and pairs with `fsync-cli` on a local backend |

## Build
Needs rustup, VS 2022 Build Tools (C++ x64 + ARM64), Strawberry Perl + NASM (vendored OpenSSL),
LLVM clang for ARM64 (`ring`; the script also looks in `%ProgramFiles%\LLVM\bin`), .NET 8 SDK,
`uniffi-bindgen-cs` v0.10.0+v0.29.4 (see `scripts/gen-bindings-cs.ps1`).

```powershell
scripts/build-windows-dll.ps1                        # x64 + ARM64 DLLs + regenerated C#
dotnet test apps/windows/tests/Fsync.CoreBindings.Tests -c Release
dotnet test apps/windows/tests/Fsync.App.Tests -c Release
dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64   # or ARM64 / win-arm64
```

## Run
Environment (same names as `fsync-cli`): `FSYNC_API` (default `http://127.0.0.1:8080`; plain http
only for loopback), `FSYNC_RELAY`, `FSYNC_DEVICE_NAME` (default: computer name), `FSYNC_DATA_DIR`
(default `%LOCALAPPDATA%\fsync`; one instance per data dir, so a second dir runs a second device).
`--background` starts in the tray without opening the window (used by start with Windows).

Files in the data dir: `secrets.dat` (DPAPI CurrentUser: device secret, history key, session
tokens, email) and `core/` (engine data: encrypted history, blobs).

## Rules
- Engine calls block: go through `EngineBridge.RunAsync` (thread pool). Core callbacks arrive on a
  Rust thread; `EngineDelegateAdapter` persists tokens there and posts the rest to the UI thread.
- Never show `FsyncException.Message` (bindgen-cs text); use `UserMessages.For`.
