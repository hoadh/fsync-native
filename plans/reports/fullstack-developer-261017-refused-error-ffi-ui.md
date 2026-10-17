# Refused error to UI (FsyncError::PeerRefused, Published.refused)

## Done
- Core: svc `FsyncError::PeerRefused`; `CoreError::PeerRefused` maps to it (NotPaired = no local paired device only). Single-target push verdict extracted to `push_verdict` (service/clips.rs).
- FFI: `FsyncError::PeerRefused` appended last (uniffi index 15); `Published.refused` added, no longer folded into `failed`.
- CLI: `clip put` all-refused bails with PeerRefused; `send` surfaces the new error text ("the other device removed this one") through the existing error path.
- Swift: bindings regenerated (build-xcframework.sh). ErrorText: "That device removed this Mac. Pair again to send to it."; MappingTests updated.
- C#: bindings regenerated with uniffi-bindgen-cs v0.10.0+v0.29.4 on Mac; diff only new field + variant, LF kept. UserMessages: "That device removed this PC. Pair again to send to it."; new UserMessagesTests.cs.
- docs/ffi-contract.md: Published, FsyncError, additive-since-v2 list, semantics line on refusal.

## Tests
- fmt, clippy -D warnings, cargo test --workspace: green.
- New: `push_verdict` unit test (refused / offline / delivered / other peer refused), FFI roundtrip count 15, `Published` keeps refused apart.
- Swift: FsyncCoreKit 0 failures; FsyncMac 231 run, 18 skipped, 0 failures.
- C#: not built here.

## Unresolved
- Device name not in text: neither app calls send_files/reads Published yet, only the error mapping exists; no name available at ErrorText level.
- No end-to-end service-level test (needs backend harness + paired devices); engine-level refusal already covered by clip_refusal_feedback.rs, verdict covered by unit test.
