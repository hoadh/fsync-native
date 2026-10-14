# fsync for Windows

Unpackaged, self-contained WinUI 3 tray app (Windows 10 22H2+ / 11, x64 + ARM64). Phase 13:
tray shell, email sign-in, pairing, device list, start with Windows. Phase 14: clipboard sync
(auto / manual + hotkeys), virtual-file paste, files (Send to, drag-drop, offers), history, settings.

| Project | What |
|---|---|
| `src/Fsync.CoreBindings` | C# bindings for `fsync_ffi.dll` (uniffi-bindgen-cs, ADR 008) + native DLL per RID |
| `src/Fsync.App.Core` | No WinUI: `EngineBridge`, DPAPI `SecretStore`, `Clipboard/` (service, mapper), `Files/` (Send to, transfers), view models |
| `src/Fsync.App` | WinUI 3 shell: `Program` (single instance), `App`, `TrayHost`, `MainWindow`, `Views/` |
| `src/Fsync.Interop` | Win32 / COM: clipboard listener + hotkeys, STA owner thread, skip rules, CF_HTML / CF_HDROP, DIB↔PNG, `ClipDataObject` (delayed render + virtual files), `.lnk` (ADR 010, 013) |
| `tests/Fsync.CoreBindings.Tests` | contract tests against the real DLL |
| `tests/Fsync.App.Tests` | bridge / view models (phase 13 fake engine); phase 14 on the real engine and the real clipboard |
| `tests/Fsync.Interop.Tests` | real clipboard, real shell paste (via `Fsync.Interop.PasteHelper`), WIC codecs |
| `tests/e2e/E2eHost.psm1`, `ClipTools.psm1`, `Uia.psm1` | shared e2e harness: Postgres (docker or `FSYNC_E2E_PG_BIN`), local relay, backend, CLI devices, app instances |
| `tests/e2e/pair-with-cli.ps1` | UI Automation: app signs in and pairs with `fsync-cli` on a local backend |
| `tests/e2e/clipboard-with-cli.ps1` | clipboard both ways, sensitive skip, history, files both ways, Send to, virtual-file paste between two app instances |

## Build
Needs rustup, VS 2022 Build Tools (C++ x64 + ARM64), Strawberry Perl + NASM (vendored OpenSSL),
LLVM clang for ARM64 (`ring`; the script also looks in `%ProgramFiles%\LLVM\bin`), .NET 8 SDK,
`uniffi-bindgen-cs` v0.10.0+v0.29.4 (see `scripts/gen-bindings-cs.ps1`).

```powershell
scripts/build-windows-dll.ps1                        # x64 + ARM64 DLLs + regenerated C#
                                                     # (the DLLs are gitignored: rebuild them whenever Generated/fsync_ffi.cs changes, or record decoding fails)
dotnet test apps/windows/tests/Fsync.CoreBindings.Tests -c Release
dotnet test apps/windows/tests/Fsync.App.Tests -c Release
dotnet test apps/windows/Fsync.sln                    # all tests; clipboard tests use the real clipboard
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
- Clipboard tests and the e2e scripts overwrite the clipboard. The e2e scripts need an interactive
  desktop (clipboard, UI Automation, hotkeys); from SSH, run them through a scheduled task with
  `/IT`. `FSYNC_DOWNLOAD_DIR` overrides where accepted files go (default `Downloads\fsync`).
- E2E prerequisites: `cargo build --release -p fsync-backend -p fsync-cli`, the relay spike
  (`cargo build --release --manifest-path spikes/iroh-discovery/Cargo.toml --target-dir target/spike`),
  the app (x64 Release) and `tests/Fsync.Interop.PasteHelper` (Release).
