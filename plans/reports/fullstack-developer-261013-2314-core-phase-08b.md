# Phase 08 part B: facade, FFI v1, Swift package, CLI, P0 exit demo

Status: DONE_WITH_CONCERNS

## Delivered
- `fsync-core/src/service/` (`FsyncService`): login/session, account (genesis / pairing needed), pairing both sides (mandatory SAS, offer modes Code|Qr), devices/remove, publish/send/fetch/read_file_range/cancel, history/settings (live `max_clip_bytes`, retention), events -> delegate on dedicated "fsync-callbacks" thread. Small core edits: `Node` relay-only mode (`force_relay`), `EngineEvent::FileServed` (sender completion), `ApiClient::set_session`, `Engine::set_max_clip_bytes`.
- `core/fsync-ffi`: UniFFI 0.29.4 (pinned), contract v1 in `docs/ffi-contract.md`; thin adapters only.
- `scripts/gen-bindings.sh`, `scripts/build-xcframework.sh`: universal arm64 + x86_64 xcframework built and linked (`lipo -info`: x86_64 arm64).
- `apps/macos/Packages/FsyncCoreKit`: SwiftPM package (generated Swift + xcframework binary target + XCTests).
- `core/fsync-cli` (clap, anyhow, JSON lines on stdout): login, pair offer|join, devices, clip put|watch, send, fetch, history, bench. Keys/session 0600 in data dir.
- `tests/e2e/p0-exit-demo.sh`; `docs/adr/008-ffi-bindings.md`; `docs/ffi-contract.md`.

## Results
- `cargo test --workspace`: pass. `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all --check`: clean. All src files < 200 lines; no unwrap/expect outside tests in new libs.
- `swift build` + `swift test` (FsyncCoreKit): 5/5 pass (contract version; settings/history/devices on fresh engine; typed errors; delegate callback off main thread; 10 MiB roundtrip).
- 10 MiB `diag_echo` roundtrip: 37.8-38.6 ms best (Swift debug build; phase-03 gate 20 ms missed in debug; mitigated by path/chunk based APIs, re-measure in release).
- P0 exit demo: PASS (run 3 times; first failed on a CLI session-file write race, fixed). Everything it starts (Postgres container, backend, relay, CLIs) is stopped; verified no leftovers.

Demo run (macOS arm64 loopback, one host, local relay; 256 MiB random file, sha256 + cmp verified):

| Round | clip A->B | file send (offer -> receiver done) | file receive (fetch only) | path |
|---|---|---|---|---|
| direct (mDNS/LAN) | 1.2 s | 5.7 s = 44.6 MiB/s | 7.8 s = 33.0 MiB/s | Lan |
| `--force-relay` | 20.4 s | 22.7 s = 11.3 MiB/s | 4.2 s = 60.9 MiB/s | Relay |

Send time includes waiting for the receiver to start fetching, so receive figures are the cleaner throughput. SAS words matched on both sides.

## Deviations / decisions
- Pairing window supports one secret at a time -> `pairing_offer(mode)`.
- `pairing_join` returns the SAS (`PairingPeer`) once ready; offer side gets `ConfirmRequest` event.
- Sender completion relies on new `FileServed` event; cancelling a send only stops tracking (offer cannot be withdrawn).
- Direction used by demo: new device B offers, existing A joins (matches core API).
- CLI `login --send-only` added so scripts can read the dev OTP between steps.
- Pinned UniFFI 0.29.4 = uniffi-bindgen-cs v0.10.0 pairing (from its README; not evaluated here).

## Concerns
- Windows/C# half of phase 03 NOT done (bindgen-cs, dll, 10k callback stress) - stays open in ADR 008.
- x86_64 slice linked but never executed (no Intel run).
- Relay-only clip latency is ~20 s (matches presence heartbeat 20 s): likely the relay URL reaches the backend only on the next heartbeat after the relay home becomes ready. Not fixed; worth an immediate presence update when relay URL appears.
- TrustReset confirm not exposed in FFI (YAGNI). `logout` also clears pinned trust.
- Sync methods must not be called from tokio workers; dropping `FsyncService` inside an async runtime would panic (`block_on`).
- Two-device pairing through the service is verified only by the e2e demo (integration test covers single device).

## Unresolved questions
- Should presence send an update as soon as the relay URL is known (fix the 20 s)?
- Is 10 MiB < 20 ms required in a release Swift build, or is the path/chunk design sufficient?

## Review round 3 fixes

Source: plans/reports/code-reviewer-261014-0100-core-08b-review.md (8.8). Paths relative to core/fsync-core unless noted.

| Item | Status | Where |
|---|---|---|
| H1 Hello only when relay ready; re-send on addr change | Fixed. iroh 1.3 `Endpoint::online()` + `watch_addr()` (n0-watcher). Hello waits <= 3 s for relay_url, then re-sent on every address change (200 ms debounce, max 8 Hello/60 s vs server 10/min) | src/net/node_addrs.rs, src/net/node.rs, src/api/presence.rs, src/api/presence_session.rs, src/api/hello_pacer.rs; tests/presence_hello.rs (2 tests) |
| M1 pairing window sync + guard + `pairing_cancel` | Fixed. Window opened synchronously, errors returned; `OpenPairing` guard closes own window (by id) on drop/abort/stop; cancel in service, FFI, CLI, docs | src/pair/window_guard.rs, src/pair/window.rs, src/service/pairing.rs, src/service/pairing_control.rs, ../fsync-ffi/src/engine.rs, ../fsync-cli/src/cmd_pair.rs, docs/ffi-contract.md; tests/service_facade.rs |
| M2 sensitive drop honours refs | Fixed. `drop_blob` -> `Shared::remove_blobs` (history + live outbox refs) | src/engine/place.rs, src/engine/shared.rs; tests/sensitive_blob_refs.rs |
| M3 Drop/run inside runtime | Fixed. `run`/`start` return `Internal` inside a runtime; Drop tears down on helper thread; documented | src/service/mod.rs, docs/ffi-contract.md; test `service_is_safe_inside_a_runtime` |
| M4 WS liveness | Fixed. WS Ping each 20 s beat; any inbound frame (incl. pong) stamps `Seen`; idle > 40 s -> reconnect. Backend verified: pings answered, reset heartbeat deadline, not rate-limited. No unit test (needs 40 s wait) | src/api/ws_util.rs, src/api/presence_session.rs; backend/src/presence/ws.rs (read only) |
| M5 TrustReset confirm API | Deferred (as instructed). Tracked for follow-up; `logout` still clears pinned trust | - |
| Low: read_file_range clamp 64 MiB | Fixed | src/service/clips.rs |
| Low: Echo outcome for explicit send | Fixed. Explicit push skips echo guard; non-Sent returns `Internal("push was suppressed as an echo")` | src/clip/send.rs, src/service/clips.rs |
| Low: presence restart only when needed | Fixed. `ensure_presence()` | src/service/mod.rs, src/service/pairing_events.rs |
| Low: pairing decision tied to request id | Fixed via FFI: `PairingPeer.request_id`, `pairing_confirm(request_id, accept)`, stale id -> Invalid | src/service/{types,pairing,pairing_control}.rs, ../fsync-ffi/src/{engine,types_account}.rs, ../fsync-cli/src/cmd_pair.rs, docs/ffi-contract.md |
| Low: truncate_part errors on extend | Fixed | src/blobs/fs_store.rs; tests/blob_contract.rs |
| Low: export temp name <= NAME_MAX | Fixed. Temp name `.{16hex}.fsync-part`; candidate stem trimmed to 255 bytes | src/blobs/export.rs |
| Low: cap bi-stream handler tasks | CORRECTION: round 3 claimed Fixed but the semaphore was missing from the code. Actually implemented in round 4 (see below) | src/file/fetch_server.rs |
| Low: `sends` TTL | Fixed. 70 min, Failed event | src/service/transfers.rs; src/service/transfers/tests.rs |
| Low: diag_echo | Removed (test-only). Swift test replaced by `testLargeDataCrossesTheBoundary` (10 MiB -> typed Invalid, asserted < 2 s) | ../fsync-ffi/src/lib.rs, docs/ffi-contract.md, docs/adr/008-ffi-bindings.md, apps/macos/Packages/FsyncCoreKit/Tests/FsyncCoreKitTests/FsyncCoreKitTests.swift |
| CLI: session.rs cross-platform + write_private_atomic | Fixed (no `std::os::unix`; `fsync_core::{ensure_private_dir, write_private_atomic}`) | ../fsync-cli/src/session.rs, src/lib.rs |
| CLI: 2 s sleep hack | Fixed. `Ctx::settle_presence(max)` | ../fsync-cli/src/ctx.rs, cmd_account.rs |
| CLI: `clip put` retries | Fixed. Waits for presence, then max 3 publishes; documented | ../fsync-cli/src/cmd_clip.rs |
| e2e: assert `via` + clip latency < 5 s | Fixed (`CLIP_MAX_S`, default 5; via Lan direct / Relay forced) | tests/e2e/p0-exit-demo.sh |

Unchanged / deferred (low): default `--api` still needs `--insecure-http`; `--yes` stays harness-only; Swift xcframework/generated sources stay gitignored (run scripts/build-xcframework.sh); gen-bindings.sh is macOS-only (.dylib); no Swift start/stop or two-device test; no unit test for 40 s liveness timeout.

Gates: fmt clean, clippy `-D warnings` clean, `cargo test --workspace` all pass (53 result lines ok), xcframework rebuilt, `swift build` ok, `swift test` 5/5.

### E2E timings (round 3)
| Round | clip A->B | file 256 MiB send / recv | via |
|---|---|---|---|
| direct | 4.05 s | 50.2 / 37.5 MiB/s | Lan |
| relay-only | 0.28 s (was 20.4 s) | 119.1 / 58.8 MiB/s | Relay |

Note: direct clip 4.05 s is the first clip after pairing (cold start + presence settle in `clip put`); bound is 5 s, so margin is thin.

## Review round 4 fixes

Re-review R2 (9.2). Each claim below was grepped against the code. Paths relative to core/fsync-core.

| Item | Status | Where |
|---|---|---|
| R2-1 direct clip 4.05 s | Fixed. Measured breakdown (new `settle_ms` in `clip put`): A's presence settle was ~0.2-0.4 s with the pre-wait cut to 300 ms; delivery after the first publish is 0.03 s. `RELAY_READY_WAIT` 3 s -> 300 ms: the Hello still follows address changes, and the wait returns at once when the relay URL is already known, so healthy reconnects pay nothing (only a reconnect with the relay down pays up to 300 ms). The CLI has no tracing subscriber, so a RUST_LOG run was not possible; the cause was confirmed by the settle_ms/latency split instead and the earlier 4.05 s was start-up + login + presence settle inside the timed window | src/api/presence_session.rs:24; tests/presence_hello.rs (relay within grace is in the first Hello; without relay Hello goes out after 250 ms - 2 s) |
| R2-1 e2e measurement | Fixed. Latency = B's `clip` event `ts_ms` minus A's `first_publish_ms` (both unix ms, same host); bound `CLIP_MAX_S` default 5 s now applies to delivery only; `settle_ms` and `attempts` printed | ../fsync-cli/src/cmd_clip.rs, tests/e2e/p0-exit-demo.sh |
| R2-2 stream cap | Fixed (really). `Semaphore` of `MAX_STREAMS_PER_CONN` = 16 acquired before `accept_bi`, permit held by the handler task. Test: 16 stalled streams hold all slots, the 17th request is served only after the 10 s request timeout (idle control answers at once) | src/file/fetch_server.rs:31,45; tests/file_stream_cap.rs |
| Low: check before `create_pairing` | Fixed. `Node::pairing_busy()` (same predicate as `open`) checked before the backend call | src/pair/window.rs (`is_busy`), src/net/node.rs:111, src/service/pairing.rs:41 |
| Low: retention size-cap loop | Fixed. Stops when an iteration frees 0 bytes (protected or shared blobs); note that an oldest inline-only item therefore also ends the cap pass | src/history/retention.rs:116 |
| Low: pin/delete unknown id | Fixed. New `FsyncError::NotFound` (service + FFI variant, mapped both ways); docs updated. Test `pin_and_delete_of_an_unknown_item_are_not_found` | src/service/{error,history_api}.rs, ../fsync-ffi/src/error.rs, docs/ffi-contract.md:44,73, tests/service_facade.rs |
| Low: reps/inline length validation | Already validated, no code change: `offer.validate()` (receive.rs:34) rejects `inline.len() != reps.len()` in fsync-proto; added the missing test | ../fsync-proto/tests/names_and_caps.rs |
| Low: keep only newest `history.corrupt-*` | Fixed. `prune_backups` after each move-aside (matches `-wal`/`-shm` of the same ts). Test `only_the_newest_corrupt_backup_is_kept` | src/history/db.rs:94-100, tests/history_db.rs |

Extra fix found by the e2e (not in the review): the first round-4 e2e run failed in the relay-only round because `send_one` returned after `tx.finish()` without waiting for the peer's ack, so a CLI that exits right after `publish` could cut the connection while the frame was still in flight on the slower relay path (receiver saw "offer stream failed", sender reported delivered). `send_one` now waits for `tx.stopped()` (None = fully acked; Some = refused -> error, retried on a fresh connection). src/clip/send.rs:110.

Gates: fmt clean; clippy `-D warnings` clean; `cargo test --workspace` all pass (56 result lines ok, none failed); xcframework rebuilt; `swift build` ok; `swift test` 5/5; no src file over 200 lines.

### E2E timings (round 4)
| Round | clip A->B (first publish to receipt) | A presence settle | file 256 MiB send / recv | via |
|---|---|---|---|---|
| direct | 0.03 s | 425 ms | 42.8 / 29.9 MiB/s | Lan |
| relay-only | 0.03 s | 206 ms | 105.4 / 52.0 MiB/s | Relay |

Another run earlier in this round (before the ack fix): direct 0.03 s (settle 308 ms), then the relay-only failure described above. Direct file speed varies run to run (42-50 MiB/s send).

## Review round 5 fixes

| Item | Status | Where |
|---|---|---|
| R3-1 backups never lose data | fixed | `core/fsync-core/src/history/recovery.rs` (`prune_backups`: keep newest `KEEP_BACKUPS`=3; older ones deleted only if footprint <= fresh DB, i.e. no data) |
| R3-1 no move-aside on wrong key | fixed | `history/db.rs`: key verdict from `history.keycheck` (blake3 keyed tag, constant-time compare). Mismatch -> `CoreError::HistoryKeyMismatch` / `FsyncError::HistoryKeyMismatch` (service + FFI), nothing moved, no fresh DB, no HistoryReset. Only unreadable DB with Same/Unknown verdict is moved aside + `HistoryReset`. Right key still recovers the original. |
| R3-1 tests | updated | `tests/history_db.rs` (wrong key then right key recovers; data backups never pruned, empty ones capped at 3), `tests/service_facade.rs` (mismatch error, then damaged file -> HistoryReset on `fsync-callbacks`), Swift `testDelegateCallbackArrivesOffTheMainThread` |
| R3-2 send_one | fixed | `clip/send.rs`: per-attempt limit (first = min(send_timeout, 2 s)); on Net error/timeout forget cached conn + close, retry once fresh; peer refusal -> `CoreError::Invalid`, non-retryable, conn kept; "delivered" = transport acked frame (documented in code and `docs/ffi-contract.md`) |
| R3-2 dedup | verified, no change | `clip/receive.rs::on_offer` calls `first_time(item_id)` before echo mark, history record, event |
| R3-4 size cap | fixed | `history/retention.rs` (`OLDEST_WITH_OWN_BLOB`; skips items freeing nothing); test `size_cap_skips_items_that_free_nothing` |
| R3-5 faster stream-cap test | skipped | still ~10 s (waits REQ_TIMEOUT); no test hook added (YAGNI) |
| (a) relay-only clip timeout at 00:59 | cause fixed earlier | snapshot predates the `tx.stopped()` ack wait in `send_one` (CLI exited before the frame was delivered); 2 consecutive e2e runs pass with current code |
| (b) relative CARGO_TARGET_DIR | fixed | `tests/e2e/p0-exit-demo.sh` makes `BUILD` absolute; run 1 used a relative dir, run 2 absolute |
| tester tests | kept passing | `core/fsync-ffi/tests/error_mapping_roundtrip.rs`, `core/fsync-cli/tests/cli_surface.rs` |
| docs | updated | `docs/ffi-contract.md` (HistoryKeyMismatch, HistoryReset semantics, delivered), `docs/adr/011-history-encryption-at-rest.md` |
| M5 TrustReset confirm API | deferred | more relevant now: trust table lives in history DB; also no app API to abandon an old history after HistoryKeyMismatch |

Gates: fmt, clippy -D warnings, cargo test --workspace (56 ok result lines, 0 failed), no src file >200 lines, xcframework rebuild, swift build, swift test (5 pass).

E2E (2 consecutive runs, both PASS):
- run1 (relative target dir): direct clip 0.02 s (settle 323 ms, attempts 1), file 47.4 MiB/s send, via=Lan; relay clip 0.04 s, file 108.8 MiB/s send, via=Relay.
- run2 (absolute): direct clip 0.02 s (settle 324 ms), file 55.2 MiB/s, via=Lan; relay clip 0.03 s, file 107.7 MiB/s, via=Relay.
No leftover processes or fsync-e2e containers.

## Review round 6 fixes

| Item | Status | Where |
|---|---|---|
| R4-1 prune only on reset | fixed | `history/db.rs`: `recovery::prune_backups` is called only in the move-aside branch (fresh DB is empty there); removed from the normal open path. Test `a_big_live_database_never_causes_data_backups_to_be_pruned` (5 data backups, 60-item live DB, 3 reopens, none deleted) and reworked `backups_with_data_are_never_pruned_and_empty_ones_keep_the_newest_three` in `tests/history_db.rs` |
| R4-2 explicit reset | fixed | `history/recovery.rs::reset_history` (moves `history.db*` aside, removes `history.keycheck`, missing dir = Ok); `FsyncService::reset_history` in `service/mod.rs`; FFI free fn `reset_history(data_dir)` in `fsync-ffi/src/lib.rs`; CLI `fsync-cli history reset --yes` (`args.rs` HistoryArgs/HistoryCmd, `cmd_account.rs::history_reset`; runs before the service opens; without `--yes` exit 1, nothing touched); docs in `docs/ffi-contract.md` incl. warning that pinned trust in the history is dropped (re-pair / trust reset) |
| R4-2 tests | added | Rust: `reset_history_clears_a_key_mismatch_and_a_new_key_opens` (`tests/history_db.rs`), service_facade mismatch -> reset -> new open; CLI `history_reset_needs_yes_and_moves_the_history_aside` (`fsync-cli/tests/cli_surface.rs`); Swift `testResetHistoryRecoversFromAKeyMismatch` |
| R4-3 one deadline | fixed | `clip/send.rs::attempt`: single `tokio::time::timeout_at` deadline covers connect and send+ack. Bound documented on `send_one` and in `docs/ffi-contract.md`: 2 s + `send_timeout` (5 s) = about 7 s per peer, peers concurrent, so also the `publish` bound |
| R4-4 error roundtrip | fixed | `fsync-ffi/tests/error_mapping_roundtrip.rs`: samples for `HistoryKeyMismatch` and `NotFound`; exhaustive `variant_index` match (compile error on a new variant) plus `VARIANT_COUNT` assertion (fails on a missing sample) |
| R4-6 outbox-protected blobs | fixed | `history/queries.rs::load_guarded` fills `temp.guarded`; `OLDEST_WITH_OWN_BLOB` in `retention.rs` excludes those hashes; test `size_cap_leaves_items_whose_blob_the_outbox_protects` |
| R4-5, R4-7 | unchanged | noted by reviewer as cost / acceptable |
| M5 TrustReset confirm API | deferred | explicit `reset_history` now gives the app a way out; trust-reset confirmation still follow-up |

Residual: pruning compares against the empty-DB footprint, so a tiny backup holding only a handful of rows could in theory be no larger than an empty database; it is only considered after a move-aside.

Gates: fmt --check, clippy -D warnings, `cargo test --workspace` (56 ok result lines, 0 failed), no src file >200 lines, xcframework rebuild, swift build, swift test (6 pass).
E2E (once, PASS): direct clip 0.03 s (settle 414 ms, attempts 1), file 54.5 MiB/s send, via=Lan; relay-only clip 0.03 s, file 106.1 MiB/s send, via=Relay. No leftover fsync processes or fsync-e2e containers.

## Review round 7 fixes

| Item | Status | Where |
|---|---|---|
| Re-pair after `reset_history` (node id still listed, `DuplicateDevice`) | fixed | `core/fsync-core/src/pair/initiator.rs`: after mutual SAS accept (both sides' consent frames, unchanged), `listed_head(trust, &n_id)` (`pair/initiator_steps.rs`) returns the pinned head when the joining node id is already in the pinned list; the initiator then skips `publish_new_list` and `trust.ingest`, and sends that head in `SignedList` exactly as after a publish. N pins it and checks membership as before. Otherwise the publish path is untouched. Name/platform updates skipped (YAGNI). |
| Tests | added | `core/fsync-core/tests/pairing_repair_after_reset.rs`: pair A+B, B gets same identity with empty trust (state after reset), re-pair succeeds, SAS matches, head seq stays 1, backend still holds 2 lists, B trusts A; second test: N declining blocks re-pairing and pins nothing. Harness: `tests/common/pairing.rs` gained `pair_on` (session against an existing backend; `pair_decisions` now wraps it). |

Notes: the test models "reset + new key" at the trust/identity level with the in-process harness (empty `TrustManager`, same node id) rather than a full `FsyncService` with a real backend, to avoid Docker. Security: nothing is published or trusted without both users accepting the SAS; a head already pinned by E is by definition one E trusts.
Gates run: fmt --check, clippy -D warnings (workspace, all targets), `cargo test -p fsync-core`. Backend untouched; e2e not run (tester running it).
