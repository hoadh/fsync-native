# ADR 008 — FFI bindings: UniFFI 0.29.4, sync methods + callback interface
Status: accepted. macOS/Swift and Windows/C# (uniffi-bindgen-cs, x64) verified. The Windows ARM64
DLL builds (clang needed by `ring`) but has not run on ARM64 hardware.

Implementation: `core/fsync-ffi` (thin adapters over `fsync_core::service::FsyncService`),
contract in `docs/ffi-contract.md` (v1), scripts `scripts/gen-bindings.sh` and
`scripts/build-xcframework.sh`, package `apps/macos/Packages/FsyncCoreKit`. Windows:
`scripts/gen-bindings-cs.ps1`, `scripts/build-windows-dll.ps1`, project
`apps/windows/src/Fsync.CoreBindings`, spike `spikes/ffi-bindings` (stress + buffer timing).

## Decision
1. UniFFI proc-macros, pinned `uniffi = "=0.29.4"`. Reason: uniffi-bindgen-cs (NordSecurity) lists
   v0.10.0 <-> uniffi-rs 0.29.4 (v0.11.0 <-> 0.31.0, v0.9.0 <-> 0.28.3). 0.29.4 is the newest
   release in the 0.29 line, so Swift and C# can share one UniFFI version. uniffi-bindgen-cs is a git
   crate (not on crates.io): `cargo install uniffi-bindgen-cs --git
   https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.10.0+v0.29.4` (commit e4f18be).
   Verified on Windows, see below; `gen-bindings-cs.ps1` refuses any other version.
2. Sync methods + callback interface (`EngineDelegate`), no `async fn` across the boundary:
   bindgen-cs async support is the weakest part of it, and sync + callbacks works in both languages.
   The Rust side owns a tokio runtime; blocking methods run `block_on` on it.
3. Callbacks are delivered from ONE dedicated thread ("fsync-callbacks") instead of tokio
   workers: ordered, panic-isolated, and callbacks may call blocking engine methods without
   deadlocking the runtime.
4. Large payloads: `Vec<u8>` is copied across the boundary (see timing); big content uses paths
   (`send_files`, `fetch_file_to`) or `read_file_range` chunks, never a single huge buffer.
5. macOS artifact: static lib per triple (aarch64 + x86_64 apple-darwin, deployment target 13.0),
   `lipo` universal, `xcodebuild -create-xcframework`, wrapped as a SwiftPM binary target.

## Verified here (macOS arm64 host, Xcode, Swift 6.4)
| Check | Result |
|---|---|
| Swift bindings generate (library mode) | ok |
| Universal xcframework | `lipo -info`: x86_64 arm64 (cross-build of vendored SQLCipher/OpenSSL works) |
| `swift build` + `swift test` (FsyncCoreKit) | 5 tests pass: contract version, settings/history/devices on a fresh engine, typed errors (Auth, Invalid, bad key length), delegate callback off the main thread, 10 MiB `Data` into Rust returns a typed error |
| x86_64 slice executed | NOT run (no Intel host / Rosetta run of the tests); only linked |
| 10 MiB `Data` roundtrip (measured once with a benchmark-only `diag_echo`, since removed from the release surface; Swift debug build, Rust release lib) | best of runs: 37.8 ms (gate in phase 03: < 20 ms) |

The 10 MiB number misses the phase-03 gate of 20 ms in a Swift DEBUG build (bounds-checked
`Data` <-> `RustBuffer` conversion dominates). It is two copies each way. Mitigation already in
the design: bulk data goes by path or in `read_file_range` chunks (<= a few MiB), so the cost is
irrelevant for the clipboard (<= max_clip_bytes, typically small) and files. Re-measure in a
release build when the app exists.

## Verified here (Windows 11 x64 host, VS 2022 Build Tools 17.14, .NET 8.0.426, 2026-10-14)
| Check | Result |
|---|---|
| `fsync_ffi.dll` x86_64-pc-windows-msvc (vendored SQLCipher/OpenSSL via Strawberry Perl + NASM) | builds, 25 MB release; ~10 min cold (OpenSSL under nmake) |
| uniffi-bindgen-cs v0.10.0+v0.29.4 on contract v2 (library mode) | generates 4.6k lines; compiles with `TreatWarningsAsErrors` (nullable warnings in the generated file suppressed). Every type maps: records -> C# `record`, enums with data -> nested records, `FsyncError` -> `FsyncException.<Case>`, delegate -> `EngineDelegate` interface |
| xUnit contract tests (`apps/windows/tests/Fsync.CoreBindings.Tests`, real DLL) | 9/9 pass: the 7 Swift tests ported, plus typed errors on an unstarted engine and native load from `runtimes/win-x64/native` |
| Callback from a Rust thread into C# (spike `ffi-bindings`, `stress_callbacks`) | 10k x 16 B: 8 ms; 10k x 4 KiB: 34 ms; in order, never on the calling thread; 20 x 1k with `GC.Collect` between: no lost or crashed callbacks |
| Buffer round trip C# -> Rust -> C# (`echo`, best of 10, Release) | 1 MiB 2.7 ms, 4 MiB 11 ms, 10 MiB 27 ms. Misses the 20 ms gate like Swift (4 copies incl. `RustBuffer` alloc); linear, ~1.35 ms per MiB per direction |
| ARM64 DLL (aarch64-pc-windows-msvc, LLVM clang 23.1.3 for `ring`) | builds, 20 MB, PE machine 0xAA64; the WinUI app builds for win-arm64. NOT executed (no ARM64 host) |

Decision for Windows: **uniffi-bindgen-cs, no C ABI fallback.** The 10 MiB miss is accepted for the
same reason as on macOS: bulk data goes by path or `read_file_range` chunks (1 MiB = ~1.4 ms).

C# specifics app code must know:
- Namespace `Fsync.Core`, free functions on `FsyncFfi` (`core/fsync-ffi/uniffi.toml`, `[bindings.csharp]`).
- Record fields keep UniFFI's camelCase (`settings.clipMode`); use `with` to change them.
- Records holding arrays (`Settings.denylist`, `ClipItem.reps`, ...) compare arrays by reference:
  do not rely on record equality for change detection in view models.
- `FsyncEngine` is `IDisposable`; dispose it (after `Stop()`) instead of waiting for the finalizer.
- `FsyncException.Message` is `"@msg=<text>"` for variants with fields and .NET's default text for
  the others: switch on the subtype and use its fields (`Network.msg`), never show `Message`.
- The generated `IFsyncEngine` interface lets view models be tested against a fake engine.
- The DLL lives at `runtimes/<rid>/native/fsync_ffi.dll`; `NativeLoader` (module initializer) resolves
  it for the process architecture, so x64 and ARM64 can ship side by side.

## Not done / open
- Windows ARM64: run the tests and the app on ARM64 hardware. The win-arm64 app output also carries
  the x64 DLL (both RID folders are copied); trim it when packaging (phase 16).
- WinUI 3 test app calling the engine: deferred to phase 13 (the bindings run in a WinUI 3 process
  there); phase 05 verified WinUI 3 + tray + clipboard on this host.
- x86_64 macOS slice still never executed.
- Swift names in the test target follow UniFFI conventions and compile against the generated file.

## Consequences
- Apps must not call engine methods from the UI thread and must marshal callbacks to it.
- Contract changes are additive; the Swift and C# files are regenerated by their scripts and
  committed (`Generated/fsync_ffi.cs`). The native DLLs are build outputs (git-ignored).
- Upgrading UniFFI means re-checking bindgen-cs compatibility first.
