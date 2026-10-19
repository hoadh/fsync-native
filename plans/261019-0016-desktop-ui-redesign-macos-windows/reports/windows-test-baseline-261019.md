# Windows test baseline on waso (2026-10-19)

- Base commit: c25c413c681211e3f32d8f293dda94c2233645b5 (main)
- Run: r261019011912-f8424d, taskMode interactive (dev logged on)
- Command: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`
- Result: 102 passed, 0 failed, 0 skipped
- **Allowed failing list: none.** Any failure in a waso phase gate = fail.

## fsync_ffi.dll (gitignored) in both Rig clones
Copied from the owner's clone `C:\Users\dev\workspace\fsync-native` (built 2026-10-17; no core/Cargo.lock/CoreBindings change between 4609dd5 and base) into
`apps\windows\src\Fsync.CoreBindings\runtimes\<rid>\native` of `rig-fsync-native` and `rig-fsync-native-rig-test`.
- win-x64 sha256: 0a13b20dfa94e4b6791a50f5bbb4c700f6cf41203937c0154ee5ac77cfd02676
- win-arm64 sha256: 2a17b494521480a17fa1db40478de8b14ec0f25e778047b5ac9fe8640a0ef9bc
- Re-copy (rebuild via scripts/build-windows-dll.ps1 by owner) if core/FFI changes.

## History
- Headless (S4U) earlier: 60/102 failed, DPAPI Access is denied, so switched to interactive.
- Interactive without DLL: 18/102 failed (native load), so copied the DLL.
