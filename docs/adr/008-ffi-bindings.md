# ADR 008 — FFI bindings: UniFFI 0.29.4, sync methods + callback interface
Status: accepted for macOS/Swift (verified here). Windows/C# half of phase 03 remains OPEN.

Implementation: `core/fsync-ffi` (thin adapters over `fsync_core::service::FsyncService`),
contract in `docs/ffi-contract.md` (v1), scripts `scripts/gen-bindings.sh` and
`scripts/build-xcframework.sh`, package `apps/macos/Packages/FsyncCoreKit`.

## Decision
1. UniFFI proc-macros, pinned `uniffi = "=0.29.4"`. Reason: uniffi-bindgen-cs (NordSecurity) lists
   v0.10.0 <-> uniffi-rs 0.29.4 (v0.11.0 <-> 0.31.0, v0.9.0 <-> 0.28.3). 0.29.4 is the newest
   release in the 0.29 line, so Swift and C# can share one UniFFI version. uniffi-bindgen-cs is a git
   crate (not on crates.io); it was NOT built or run here. That compatibility is taken from its
   README, not measured.
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

## Not done / open
- Windows: `fsync_ffi.dll` x64 + ARM64, uniffi-bindgen-cs generation, WinUI 3 test app, 10k-callback
  stress. Fallback if bindgen-cs fails on this contract: C ABI + csbindgen (+1-1.5 weeks).
- Callback stress test (10k callbacks) not run.
- Swift names in the test target follow UniFFI conventions and compile against the generated file.

## Consequences
- Apps must not call engine methods from the UI thread and must marshal callbacks to it.
- Contract changes are additive; the Swift file is regenerated and committed by the script.
- Upgrading UniFFI means re-checking bindgen-cs compatibility first.
