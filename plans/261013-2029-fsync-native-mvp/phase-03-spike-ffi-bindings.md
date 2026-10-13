# Phase 03 — Spike: FFI bindings (UniFFI Swift, uniffi-bindgen-cs) + FFI contract

## Context Links
- Design §2 Bindings, §5 risk "C# bindings maturity"
- Consumers: [phase-08](phase-08-core-history-ffi-cli.md), [11](phase-11-macos-app-shell-pairing.md), [13](phase-13-windows-app-shell-pairing.md)

## Overview
- Priority: P0 | Status: pending | Effort: 4-5 days (A + D pairing)
- Validate both binding toolchains with the real shape of our API (async, callbacks, byte buffers, errors) and **freeze FFI contract v0** so app teams can build UI before core is done.

## Key Insights
- uniffi-bindgen-cs (NordSecurity) lags upstream UniFFI versions → pin UniFFI version compatible with it; Swift side uses same version.
- Async across FFI: Swift async supported; C# async support in bindgen-cs may be limited → design contract with callbacks/delegate instead of async fns if needed (works on both).
- Callbacks fire on Rust threads → apps must marshal to main/UI thread (DispatchQueue.main / DispatcherQueue).

## Requirements
- Exercise: object with constructor, sync method returning `Vec<u8>`, `Result<_, FsyncError>` enum, record + enum types, callback interface invoked from tokio thread, large buffer (10 MiB) copy cost.
- Build artifacts: macOS universal xcframework (arm64+x86_64); Windows `fsync_ffi.dll` x64+ARM64.

## Architecture — FFI contract v0 (proc-macro UniFFI, pseudocode)
```
object FsyncEngine
  new(cfg: EngineConfig{data_dir, api_base, relay_url, device_name, platform, history_key: bytes32}, delegate: EngineDelegate)
  <!-- Updated: Validation Session 1 - history_key added (encrypted history at rest) -->
  start() / stop() / set_session(tokens: SessionTokens)
  // pairing
  pairing_offer() -> PairingOffer{qr_payload: String, code: String, expires_ms}
  pairing_join(input: String) -> PairingPeer{node_id, name, platform, sas_words}
  pairing_confirm(accept: bool)
  devices() -> Vec<Device{node_id, name, platform, status: Offline|Lan|Relay, last_seen_ms}>
  remove_device(node_id)
  // clipboard
  publish_clip(clip: LocalClip{reps: Vec<Rep{mime, data}>, files: Vec<String /*paths*/>}) -> ItemId
  fetch_rep(item: ItemId, idx: u32) -> Vec<u8>
  fetch_file_to(item, file_id, dest_path) -> TransferId        // mac promises, eager fallback
  read_file_range(item, file_id, offset: u64, len: u32) -> Vec<u8> // Win IStream::Read (blocking)
  send_files(node_id, paths: Vec<String>) -> TransferId
  cancel(transfer: TransferId)
  // history
  history(query: Option<String>, cursor: Option<String>, limit: u32) -> HistoryPage
  pin(item, bool) / delete(item) / clear(keep_pinned: bool)
  settings() -> Settings / set_settings(Settings{clip_mode: Auto|Manual, retention_days, retention_bytes, max_clip_bytes, denylist})
callback interface EngineDelegate
  on_remote_clip(ClipItem)  on_device_status(Device)  on_transfer(TransferEvent)
  on_incoming_files(FilesOffer)  on_pairing_event(PairingEvent)  on_error(FsyncError)
```
Errors: `FsyncError{ NotPaired, PeerOffline, Network(msg), Integrity, TooLarge, Auth, Io(msg), Cancelled }`.

## Go / No-Go
| Check | Go | Fallback |
|---|---|---|
| uniffi-bindgen-cs builds contract v0 on pinned UniFFI | all types/callbacks work in WinUI 3 test app | C ABI (`extern "C"`) + csbindgen for P/Invoke; hand-written C# wrapper (+1-1.5 wk D) |
| Callback from tokio thread → C# without crash/GC issue | 10k callbacks stress OK | queue + polling `next_event()` method |
| Swift xcframework universal | runs on arm64 + x86_64 | n/a |
| 10 MiB buffer roundtrip | < 20 ms | add file-path based API for large payloads |

## Related Code Files
- Create: `spikes/ffi-bindings/**`, `scripts/build-xcframework.sh`, `scripts/build-windows-dll.ps1`, `scripts/gen-bindings.sh`, `docs/ffi-contract.md`, `docs/adr/008-ffi-bindings.md`

## Implementation Steps
1. Stub crate implementing contract v0 with fake data (spike only).
2. Generate Swift; tiny SwiftUI app calls each method; callback stress.
3. Generate C#; tiny WinUI 3 app same tests; ARM64 build.
4. Build scripts promoted to `scripts/` (kept).
5. Publish `docs/ffi-contract.md` v0; ADR 008.

## Todo List
- [ ] Pin UniFFI version compatible with bindgen-cs
- [ ] Swift test app green
- [ ] C# test app green (or fallback decided)
- [ ] Build scripts in scripts/
- [ ] ffi-contract.md v0 + ADR 008

## Success Criteria
- Both test apps call every contract method; ADR 008 decides bindgen-cs vs C ABI.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| bindgen-cs version lag blocks UniFFI features | M x M | contract avoids async/exotic types; C ABI fallback |
| Contract churn after freeze | M x M | additive changes only; version field in docs |

## Rollback
- Spike isolated. Contract is a doc; changes via PR with both app owners reviewing.

## Security Considerations
- Session tokens passed into core and stored by apps in Keychain/Credential Locker, not in core files.

## Next Steps
- Phase 08 implements contract for real; 11/13 build UI against it.
