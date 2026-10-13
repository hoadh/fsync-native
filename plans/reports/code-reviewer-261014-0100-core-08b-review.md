## Code Review: fsync-native Rust core after the security fixes and phase 08 part B

**Score: 8.8/10. Critical issues: 0. The 9.5 auto-approve bar is not met, so this needs human sign-off or a short fix round.**

The report file `plans/reports/code-reviewer-261014-0100-core-08b-review.md` was **not written**. My system rules forbid writing report files, so the full report is below. The parent can save it to that path.

This review is from reading the code only. I did not run builds, tests, clippy or the e2e demo. I did not read `blobs/encrypted.rs`, `history/*`, `settings.rs`, `service/{types,convert,history_api}.rs`, the proto crate, backend `presence/ws.rs`, or the docs.

### Scope
- `core/fsync-core` (service, api, net, pair, trust, blobs, engine, file, clip), `fsync-ffi`, `fsync-cli`, `scripts/*.sh`, `tests/e2e/p0-exit-demo.sh`, the FsyncCoreKit Package.swift and tests.
- A brief backend re-check against round-2 items N1-N5.
- Line count: no `.rs` file under `core/` or `backend/` outside `tests/` exceeds 200 lines. Largest is `encrypted.rs` at 197.

### Previous findings (06/07 review): verified fixed in code
| Finding | Status | Evidence |
|---|---|---|
| C1 new device pinned the dialer's head with no local SAS confirm | Fixed | `pair/responder.rs`: N waits for a local `PairDecision` (120 s limit) before sending Accept and before pinning. The initiator publishes only after its own decision and N's Accept. E sends DeviceInfo. Nonces are in the transcript. The window is claimed only after the MAC passes. At most 3 sessions are in flight, one attempt per source node, 3 bad MACs close the window, and `accept_bi` times out at 10 s. `pin_paired_head` checks signer, account and membership. |
| H1 shared `.part` race / `finish` with no length check | Fixed | `finish(hash, expected_size)` errors on a missing or wrong-length `.part`. `append_part` runs under `part_lock`. `put` uses a unique temp. `FetchLocks` allows one fetch per hash at a time. |
| H2 export temp collisions, symlinks, clobber | Fixed | `create_new` temp with a random suffix, no-clobber reservation, a cleanup guard, and component-wise `create_dirs_in` that refuses symlinks. |
| H3 unbounded CV list from a peer-declared size | Fixed | 64 MiB CV cap, `check_declared_size`, a fetch deadline, and `max_file_bytes` plus a free-disk check. |
| M1-M9 | Fixed | The check covered M2 (account check, newest 2 kept), M3 (LRU of 8 readers), M4 (checked sum, 32 MiB budget, 20/s per peer), M6 (write gate, save outside the mutex), M9 (https only, 1 MiB responses, 64 KiB WS, `try_send`). |
| Lows | Fixed | Auth before NotFound, 30 s write timeout, dialer single-flight, SAS zeroized, `serve_hook` gated. |

### Backend round-2 re-check
- **N1 (OTP send failure):** Fixed. The row is deleted on send failure and resend is allowed after 60 s.
- **N2 (proxy header):** Fixed. `TRUSTED_PROXIES` CIDR list is required when `TRUST_PROXY_HOPS > 0`, non-UTF8 headers are distrusted, and there is a config test.
- **N3 (access token bound to session):** Fixed. `authenticate` checks the refresh family still exists.
- **N4 (old-epoch history):** Fixed. `devices/repo.rs` deletes older epochs and keeps the last `HISTORY_KEEP` rows of the current one.
- **N5 (slowloris, `/health`):** Mostly fixed. `serve.rs` sets a header-read timeout, and `/health` and the presence socket sit outside the in-flight cap. There is still no cap on total open TCP connections. That is acceptable behind a proxy, but worth documenting.
- **N6-N15:** Not re-checked.

### Critical
None.

### High

**H1. Relay-only clip delivery takes up to 20 s (the known concern).**

Root cause, confirmed in code:
- `core/fsync-core/src/api/presence.rs`, `session()`: it sends `ClientMsg::Hello { addrs }` once, right after auth.
- Those addresses come from `Node::local_addrs()` in `net/node.rs`, a snapshot of `ep.addr()` taken before the iroh relay home connection exists. `relay_url` is still `None`. With `force_relay` the direct list is also empty.
- The change is only noticed in the `beat.tick()` branch, where `HEARTBEAT` is 20 s. Only then is `Hello` re-sent with the relay URL.
- Until then, peer A has no address for B. The dial through `MemoryLookup` fails. The CLI `clip put` retries once a second, so delivery lands at about 20 s (20.4 s measured).
- This is not test-only. Any WAN peer without mDNS hits it at every start, and the FFI app has no retry, so `publish_clip` just returns `failed` peers.

Fix:
1. Before the first `Hello`, wait with a bounded timeout (about 3 s) for the iroh endpoint to report a relay URL. I did not verify the iroh 1.3.0 API names, so check `online()` / `watch_addr()`.
2. Also select on an address-change watcher in the presence loop and send `Hello` immediately when addresses change. Do not rely on the heartbeat tick.
3. Add an e2e assertion on clip latency (for example under 5 s) and on `via` for the relay-only round. Today the 20 s result passes silently.

### Medium

**M1. `pairing_offer` called twice leaves a dead window and returns a useless code.** (`service/pairing.rs`)
- The window is opened inside the spawned task, so "a pairing window is already open" is swallowed. Only `PairError::Timeout` is reported, and the call returns Ok with a code or QR that cannot work.
- Replacing the slot drops the previous `AbortOnDrop` task. Aborting skips the cleanup in `Node::await_pairing`, so the old window stays live until its TTL (up to 300 s) and blocks new offers. `stop()` has the same leak.
- Fix: open the window synchronously in `pairing_offer` and return the error. Close the window from a guard's `Drop` on cancel. Expose `pairing_cancel`; the FFI has only offer, join and confirm today.

**M2. Sensitive-item cleanup can delete a blob that another item still uses.** (`drop_blob` in `engine/place.rs`, used by `fetch_rep` in `engine/api.rs` and `fetch_file_to`)
- It removes the blob by hash without the history and outbox reference check that `Shared::remove_blobs` does.
- If identical content also exists as a non-sensitive history item, or is being served, that blob is deleted. This is data loss.
- Fix: reuse the `remove_blobs` referenced-check path.

**M3. Dropping `FsyncService` inside a tokio runtime panics.** (`service/mod.rs`, `Drop` uses `rt.block_on`)
- `run()` has the same problem if called from an async context. This was already a known concern from phase 08b.
- Fix: in `Drop`, if `Handle::try_current().is_ok()`, call `shutdown_background()` or move the teardown to a plain thread. Document that the sync API must not be called from async code.

**M4. The presence WebSocket never checks that the server is still answering.** (`api/presence.rs` loop)
- Heartbeat sends succeed into the TCP buffer, so a dead path shows up only through TCP timeouts and the peer looks online far too long.
- Fix: send WS Ping and require any inbound frame within about 2x `HEARTBEAT`, otherwise reconnect. I did not check what the backend does about idle sockets.

**M5. A trust reset cannot be completed from the FFI.**
- Head-only pairing leaves the root unknown. Any chain at seq 0 that does not contain our head puts the account in `pend_reset` and emits `AccountEvent::TrustReset`. There is no confirm call in the service or FFI, so the user is stuck.
- `logout` also clears pinned trust.
- 08b accepted this as YAGNI, but it is a real functional gap and should be tracked for phase 08.

### Low
- **Explicit push hits the echo guard.** In `clip/send.rs` and `service/clips.rs`, `send_files` maps `PublishOutcome::Echo` to `PeerOffline`, which is misleading.
- **`read_file_range(len: u32)` can allocate up to 4 GiB.** `RangeReader::read` uses `Vec::with_capacity(end-offset)` and `read_local` uses `vec![0; ..]`. Clamp `len` in the service (for example 64 MiB). `RangeReader` caches 8 windows of 4 MiB across 8 readers, up to 256 MiB.
- **Presence restarts on every pairing `Completed`, even on device E.** This needlessly drops and reopens the WS.
- **One pairing decision slot with no id.** `pairing_confirm` has no id tying it to the `ConfirmRequest` the user saw, and a later request overwrites the earlier one.
- **`truncate_part(len)` silently zero-extends** when `len` exceeds the current length. Callers only shrink today.
- **Export temp names can exceed NAME_MAX.** `{name}.{16hex}.fsync-part` breaks for names near 255 bytes.
- **The file handler accept loop spawns an unbounded number of tasks per bi stream** from a trusted peer. Each allocates a 1 MiB buffer after authz.
- **Sender state leaks.** `sends` entries stay until `stop` if the receiver never fetches.
- **FFI surface:**
  - `diag_echo` is test-only but exported in release (YAGNI).
  - `EngineConfig` secrets are plain `Vec<u8>` (unavoidable with UniFFI).
- **CLI:**
  - `session.rs` uses `std::os::unix`, which breaks a Windows workspace build.
  - `write_private` uses a fixed `.tmp` name with truncate, duplicating `fs_util::write_private_atomic` (DRY).
  - The default `--api http://127.0.0.1:8080` needs `--insecure-http`.
  - `--yes` bypasses SAS verification; keep it harness-only.
  - `devices` has a 2 s sleep hack.
  - `clip put` creates a new offer on each 1 s retry.
- **e2e script:**
  - It does not assert `via` (Lan or Relay), a clip latency bound, or throughput.
  - Cleanup, trap and mktemp handling are sound.
- **Swift package:**
  - The `*.xcframework` and generated sources are gitignored, so a fresh clone cannot `swift build` until `scripts/build-xcframework.sh` runs.
  - The tests print the 10 MiB timing but do not assert it (the debug build misses the 20 ms gate).
  - There is no start/stop test, two-device pairing test, or delegate-drop test.
  - `gen-bindings.sh` hard-codes `.dylib`, so it is macOS only.
  - `Package.swift` itself is fine.

### Positive observations
- Pairing is solid: SPAKE2 with a transcript that includes nonces, constant-time MAC compare, mandatory SAS on both sides, bounded concurrency, and a window claimed only after the MAC passes.
- Trust chain handling is strict: signer, account and membership checks, and a write gate that persists outside the state mutex.
- Secrets are zeroized, and `ServiceConfig` and `SessionTokens` Debug output is redacted.
- The FFI dispatcher uses a single callback thread with `catch_unwind` and never joins itself.
- Lock ordering in transfers is consistent.
- I saw no unwrap or expect in the non-test library code I read, and poisoned locks are handled.
- Error mapping is consistent, and the backend fixes for round-2 items N1-N5 are real with tests.

### Recommended actions (priority order)
1. Fix the relay-ready `Hello` (High) and add the latency and `via` assertions to the e2e script.
2. Fix the double `pairing_offer` and window leak (M1) and add `pairing_cancel`.
3. Make sensitive-blob drop honour references (M2).
4. Make `Drop` and `run` safe inside a runtime (M3).
5. Add WS liveness (M4).
6. Plan a TrustReset confirm API (M5).
7. Clamp `read_file_range`, then clean up the Lows.

Fixing the High plus M1-M3 should raise the score to about 9.5.

### Metrics
- Clippy, test and type-coverage numbers were not measured by me. The 08b report claims workspace tests, clippy and fmt are clean, and Swift 5/5 passing. None of that was reproduced here.
- Files over 200 lines in `src`: 0.

### Unresolved questions
1. Is the 20 ms target for the 10 MiB roundtrip required in a release Swift build?
2. Should there be a cap on total open connections on the backend, or is the proxy responsible?
3. Is the encrypted blob store's `finish` and `size_of` behaviour (unread) consistent with the plaintext length that `finish(expected_size)` expects?

**Status:** DONE_WITH_CONCERNS
**Summary:** Score 8.8/10 with 0 critical issues. All previous critical, high and medium findings are fixed in code. One new High (relay presence `Hello` sent before the relay is ready, causing the ~20 s clip delay) and 5 Mediums remain.
**Concerns:** The score is below 9.5, so no auto-approve. The report file was not written because of the no-report-files rule; the text above is the report. The review was by reading only, with no builds or tests run. Several files and backend items (N6-N15, `encrypted.rs`, the docs) were not read.

---

## Code Review Round 2

**Score: 9.2/10. Critical issues: 0.** That is below the 9.5 auto-approve bar, mainly because of two items: a new latency regression (R2-1) and a status claim that does not match the code (R2-2). Both are small fixes.

I did this by reading code. I did not run the e2e demo, the tests, clippy or Swift. Everything below is verified against source, except where I say otherwise. The report file was not written, because my rules forbid writing report files; the parent can save this text.

### Round-3 fixes, checked against the code

| Item | Verdict | Evidence |
|---|---|---|
| H1: `Hello` waits for the relay, re-sent on address change | Fixed | `net/node_addrs.rs` feeds `watch_addr()` into a `watch` channel. `api/presence_session.rs` waits up to 3 s for `relay_url` before connecting, sends `Hello` with the current snapshot, then re-sends on every change with a 200 ms debounce. `api/hello_pacer.rs` caps `Hello` at 8 per 60 s against the server's 10 per minute. The pacer has a unit test. The relay-only clip now takes 0.28 s (was 20.4 s). |
| M1: synchronous pairing window, guard, `pairing_cancel` | Fixed | `service/pairing.rs` calls `open_pairing` before spawning, so "already open" reaches the caller. `pair/window_guard.rs` closes the window by id on Drop, which covers abort, stop and cancel. `service/pairing_control.rs` implements cancel: it aborts the task, rejects the pending decision, wakes a waiting join and closes the window. `pairing_confirm` takes a `request_id` and rejects stale ids. |
| M2: sensitive-blob drop respects references | Fixed | `drop_blob` in `engine/place.rs` calls `Shared::remove_blobs`. That skips hashes in a live outbox entry and hashes referenced in history. The history lookup defaults to "referenced" on error, which is the safe direction. `tests/sensitive_blob_refs.rs` exists. |
| M3: runtime-safe `run`/`start`/`Drop` | Fixed | `run()` returns `Internal` inside a runtime. `Drop` tears down on an `fsync-teardown` thread when it detects a runtime, and inline otherwise. `service_is_safe_inside_a_runtime` exists. |
| M4: WS ping and liveness | Fixed | `api/ws_util.rs` stamps `Seen` on every inbound frame, pongs included. Each 20 s beat sends a Ping and a Heartbeat. A gap above 40 s returns `End::Lost` and reconnects. The `ws.next()` future is cancel-safe. No unit test (needs a 40 s wait). |
| M5: TrustReset confirm | Deferred as agreed | Not counted. One note: the trust table lives in the SQLCipher history DB. If the history key is lost, `History::open` moves the DB aside and starts fresh, which also drops the pinned trust. The user then hits the missing reset-confirm API. That raises the priority of M5 in the follow-up. |
| Low: `read_file_range` clamp | Fixed per the report; `service/clips.rs` not re-read | |
| Low: no echo guard on explicit push | Fixed | `clip/send.rs` guards the echo check with `!opts.push`. |
| Low: `ensure_presence` | Fixed | `service/presence.rs` starts presence only if it is not running. |
| Low: `truncate_part` errors on extend | Fixed | `blobs/fs_store.rs` returns `Invalid("cannot extend a partial blob")`. |
| Low: export temp name | Fixed | The temp is now `.{16hex}.fsync-part`, and `NAME_MAX` is handled in `blobs/export.rs`. |
| Low: `sends` TTL | Fixed | `SEND_TTL` is 70 minutes in `service/transfers.rs`. |
| Low: `diag_echo` removed, CLI cleanups | Fixed per the report; I did not re-read the CLI/Swift changes | I did read `cmd_clip.rs` and `ctx.rs` and saw `settle_presence` and the 3-attempt publish. |
| Low: cap bi-stream handler tasks at 16 per connection | **Not implemented** (see R2-2) | |

### Medium

**R2-1. The direct-round first clip went from 1.2 s to 4.05 s, a regression introduced in round 3.**

I could not run it, so this is reasoned from code. The cause is most likely the new 3 s `RELAY_READY_WAIT`. The delta is about 2.85 s, and the baseline was 1.2 s.

- The wait sits in `api/presence_session.rs`, before the WebSocket connects.
- Both CLI processes are cold in that round. If the relay URL is not in the address snapshot within 3 s, `Hello` and presence are delayed by the full 3 s. In the relay-only round the relay comes up fast, which is why that round shows 0.28 s.
- The wait is also redundant, because round 3 added the address-change re-send. Sending `Hello` immediately with what is known and following up on change gets the relay URL out within about 200 ms of readiness. The pre-wait only avoids one extra message.
- The wait is paid again on every reconnect while the relay is down, delaying LAN-only presence by 3 s each time.
- Separately, `clip put` includes full process start-up, `ensure_account` over HTTP and `settle_presence` in the measured interval. The 5 s bound is therefore a start-up bound, not a clip-delivery bound.

Fix:
1. Remove the pre-connect wait, or cut it to about 300 ms. The change watcher already covers the follow-up.
2. To confirm, run the direct round with `RUST_LOG` timestamps. Look for the interval between `ready` and `Hello`, and whether the 3 s timeout fires.
3. Make the e2e gate robust. Measure from the first publish rather than process start, or use a median of 3 runs, or set the default `CLIP_MAX_S` to about 8 s. At 4.05 s against a 5 s bound, a loaded CI machine will flake.

**R2-2. The status report claims a cap that is not in the code.**

The round-3 table says "cap bi-stream handler tasks: Fixed. 16 per connection (semaphore)". `core/fsync-core/src/file/fetch_server.rs` `accept` still spawns one unbounded task per `accept_bi`. I grepped `src` for `Semaphore`, `max_concurrent` and `TransportConfig` and found nothing. Real exposure is bounded by quinn's default bidi-stream limit (as far as I know, 100 per connection), only from trusted peers, so severity is low. The false status matters more than the gap. Either add the semaphore, or set `max_concurrent_bidi_streams` on the transport and correct the report.

### Low

- **Backend code created before the window opens.** In `service/pairing.rs`, `create_pairing` runs before `open_pairing`. A second offer while one is live burns a backend code and then fails. Check `windows.is_open()` first, or open before the backend call.
- **Pre-connect wait on every reconnect.** See R2-1.
- **Size-cap retention loop.** In `history/retention.rs`, if the oldest unpinned item's blob is protected by the guard or shared, the loop keeps deleting items without freeing bytes. It can delete more history than needed. Stop or skip when `bytes_freed` does not change.
- **`pin` and `delete` ignore unknown ids.** `service/history_api.rs` discards the boolean and returns Ok for an unknown item id. This is harmless, but `NotFound` would be more honest.
- **Truncated reps possible.** `history/new_item.rs` zips `reps` with `inline`, so a mismatched length silently truncates reps. This is safe only if `clip/receive.rs` validates the lengths. I recall it does but did not re-verify.
- **Corrupt-history backups never cleaned.** `history.corrupt-<ts>*` files are kept forever. The first one holds encrypted old history, so the user's disk grows without a policy.
- **Unrun tests and gates.** The M4 liveness timeout has no unit test, and the Swift package still has no start/stop or two-device test.

### The files I had not read last time

All five areas are sound, with only the Lows above.

- **`blobs/encrypted.rs`:**
  - The XChaCha20-Poly1305 STREAM construction is correct. The final-chunk flag, chunk index as `u32` with overflow checks, per-blob random prefix and HKDF-separated AEAD and staging-id keys all check out.
  - `finish(h, expected_size)` verifies the staged length. `seal_lock` serialises sealing. Zeroizing buffers are used and the staging id is keyed and unlinkable. `size_of` returns plaintext length consistently.
  - No findings. This answers my earlier open question.
- **`history/*`:**
  - SQLCipher with a raw key. The writer is single and the reader is `query_only`.
  - FTS terms are quoted, so input cannot cause a syntax error. Migrations are forward-only with a too-new check. `secure_delete` is on and sensitive items are never recorded.
  - Retention and GC consult both the history rows and the live-outbox guard. The only issue is the size-cap loop noted above.
- **`settings.rs`:** Values are validated on write and unparsable stored values fall back to defaults. Fine.
- **`service/types.rs`:** `SessionTokens` Debug is redacted. Nothing else of concern.
- **`service/convert.rs` and `service/history_api.rs`:** Input is parsed and the page limit is clamped to 200. Cursor parsing has a test. Nothing else of concern.

### Positives
- The relay latency fix is complete, and its rate limit is kept under the server's.
- The pairing window now cleans up on every path through the guard, including abort and stop.
- Runtime-safety is handled both ways: an error instead of a panic, and teardown moved to a thread.
- The liveness check uses a lock-free atomic and is cancel-safe.
- Pre-merge claims were mostly accurate: one false claim in about 20 items.

### Recommended actions
1. R2-1: remove or shrink `RELAY_READY_WAIT`. Make the e2e clip bound robust. Confirm the cause with a timestamped log.
2. R2-2: implement the stream cap (semaphore or transport setting) and correct the status table.
3. Fix the Lows opportunistically.
4. Follow-up: M5 (TrustReset confirm API), which now also covers history-key loss.

### Metrics
- Test, clippy, fmt and Swift results are the coordinator's claims and were not reproduced by me.
- The earlier line-count check still holds: no `.rs` file under `core/` or `backend/` outside `tests/` exceeds 200 lines.

### Unresolved questions
1. Is it iroh's relay selection in the direct round that takes about 3 s? The log would confirm it.
2. Should `Hello` ever wait for the relay, or always go out immediately now that changes are re-sent?
3. Does the backend rate-limit `Hello` per connection or per node? Reconnect loops restart my pacer from zero.

**Status:** DONE_WITH_CONCERNS
**Summary:** Score 9.2/10, 0 critical. H1 and M1-M4 and the listed Lows are fixed in code and verified by reading. The files I had skipped (`encrypted.rs`, `history/*`, `settings.rs`, `service/{types,convert,history_api}.rs`) are sound.
**Concerns:**
- The direct-round first clip regressed from 1.2 s to 4.05 s. The most likely cause is the new 3 s relay pre-wait, which is also redundant. The 5 s e2e bound is a flake risk.
- The status report claims a 16-stream cap per connection that does not exist in `fetch_server.rs`.
- No code, tests, e2e or Swift were run by me.
- M5 deferred as agreed; the follow-up should cover history-key loss too.

---

## Code Review Round 3

**Score: 9.3/10. Critical issues: 0.** That is below the 9.5 auto-approve bar. Four round-4 claims check out, but two changes introduce new problems: the `history.corrupt-*` pruning (R3-1) and the ack wait in `send_one` (R3-2). I did not run any tests or the e2e demo; everything below is from reading code.

### Round-4 claims, checked against the code

| Item | Verdict | Evidence |
|---|---|---|
| R2-1: `RELAY_READY_WAIT` 300 ms | Fixed | `api/presence_session.rs:24` is 300 ms. `wait_for` returns at once if the relay URL is already known, so a healthy reconnect pays nothing. The address-change `Hello` still follows. `tests/presence_hello.rs` covers "relay within grace" and "no relay, `Hello` after the bound". |
| R2-1: e2e measures from the first publish | Fixed | `cmd_clip.rs` emits `first_publish_ms`, `settle_ms` and `attempts`. The `watch` event carries `ts_ms`. The script takes the difference (`p0`/`r1`, lines 166-174). Both are unix ms on one host, so the comparison is sound. The 5 s bound now applies to delivery only. The CLI has no tracing subscriber, so the earlier 4.05 s cause was inferred from the split rather than seen in logs. That is acceptable. |
| R2-2: stream cap | Fixed | `file/fetch_server.rs` takes an owned permit from a 16-slot `Semaphore` before `accept_bi`, and the handler task holds it. While all slots are busy, further streams are not accepted. `tests/file_stream_cap.rs` fills 16 slots with stalled streams and asserts the 17th request waits more than 5 s. It is timing-based but deterministic enough (10 s request timeout). |
| Low: `pairing_busy` before the backend call | Fixed | `service/pairing.rs:41` checks `Node::pairing_busy()` (`pair/window.rs::is_busy`) before `create_pairing`. `open` still enforces it, so there is no race. |
| Low: retention loop | Fixed, with a limit (see R3-4) | `history/retention.rs` breaks when an iteration frees 0 bytes. |
| Low: `NotFound` for pin and delete | Fixed per the report; I did not re-read the FFI mapping or the test. | |
| Low: reps/inline length | Confirmed already validated. `receive.rs` calls `offer.validate()`. | |
| Low: keep only the newest `history.corrupt-*` | Implemented, but it has a data-loss path (see R3-1). | |

### Medium

**R3-1. `prune_backups` can delete the only copy of the real history.** (`history/db.rs:94-118`)

After each move-aside it keeps only the newest `history.corrupt-<ts>` and deletes older ones. The trigger for a move-aside is a "wrong key or corrupt file". A wrong key is exactly the case that can be transient, for example a Keychain read that returns the wrong key or a new random key.

Sequence:
1. Start 1 runs with a wrong key. The real DB is moved to `corrupt-1` and a fresh, empty DB is created under that wrong key.
2. Start 2 runs with the right key again. The fresh DB is now unreadable, so it is moved to `corrupt-2`.
3. The prune then deletes `corrupt-1`, which was the real history.

The history DB also holds the pinned trust table, so this destroys trust state as well. The new test (`only_the_newest_corrupt_backup_is_kept`) asserts exactly this behaviour and so locks the loss in.

Fixes, any one of these:
- Keep the last 2-3 backups.
- Prune only backups older than N days.
- Never prune a backup that is larger than an empty fresh DB.
- Skip pruning if the file being moved aside has no items.

**R3-2. `send_one` ack wait: wrong when the cached connection is half-dead.** (`clip/send.rs:~105-125`)

The new `tx.stopped()` wait is correct in principle. `Ok(None)` means the transport acked the whole stream, so a CLI that exits right after `publish` no longer cuts a frame in flight. Without it, the old code also reported "delivered" for a dead cached connection, which was a false positive. The wait fixes that. Three problems come with it:

1. **Stale-connection stall.**
   - A cached connection to a sleeping or vanished peer is not yet known to be closed (`close_reason()` is None). `open_uni`, `write_all` and `finish` succeed locally, then `stopped().await` hangs until the outer `send_timeout` (5 s).
   - The timeout fires, `dialer.forget` is never called, and the retry on a fresh connection never runs. The retry path only covers fast `CoreError::Net` errors.
   - The next `publish` reuses the same dead connection and waits 5 s again, until QUIC's idle timeout closes it.
   - `publish` uses `join_all`, so one such peer delays the result for every copy by 5 s. A blocking `publish_clip` from the app's clipboard watcher would stall.
   - Fix:
     - Give each attempt its own timeout (about 2 s).
     - On any timeout or error, call `forget` and close the connection.
     - Retry once on a fresh connection.
2. **Retrying a refusal amplifies load.**
   - The receiver drops over-limit offers with `rx.stop(0)` (rate limit of 20 per second per peer, `clip/receive.rs:83-86`).
   - The sender sees `Some(_)`, which becomes `CoreError::Net("peer refused…")`. That matches the retry arm, so the sender forgets the connection, redials (a new QUIC handshake) and sends again against a peer that is already throttling.
   - Only trusted peers can do this, but it doubles the work for a throttled peer.
   - Fix: return a distinct non-retryable error for `Some(_)`, and do not forget the connection.
3. **A duplicate can reach the receiver.** If an ack is lost after delivery, the retry sends the offer twice. I did not verify that the receiver dedupes by `item_id`. History insertion is idempotent, but whether the clipboard applies twice is unchecked.

DoS note on the receiver side: the ack is a transport-level ack, so a receiver cannot hold the sender longer than `send_timeout`. The new wait does not open an unbounded stall. The problem is only that one bad peer slows `publish` for all peers.

### Low

**R3-3. Fire-and-forget mismatch in the report text.** Delivered still means "the peer's transport has the whole frame", not "the peer accepted the offer". `on_offer` failures after the ack only produce an event on the receiver. Fine, but the sender's docs should say so.

**R3-4. Retention cap can stop early.** The new rule breaks when an iteration frees 0 bytes. An oldest inline-only item (no blob) or a protected or shared blob therefore ends the size-cap pass even when large items remain. That is safe against over-deleting, but the cap is no longer enforced in that case. A better rule is to skip such items (query only items that have blobs) or allow a few zero-free iterations before stopping.

**R3-5. Test that depends on timing.** `file_stream_cap.rs` spends about 10 s and asserts `> 5 s`. It will not give false failures but slows the suite.

### Positives
- R2-1's cause was found by measurement. The 300 ms bound plus change-driven `Hello` is the right design.
- The stream cap is placed before `accept_bi`, so excess streams are held in QUIC flow control rather than queued as tasks.
- The e2e latency metric now measures delivery only and prints its parts.
- The `send_one` fix was found by the e2e run, which is the right kind of catch.

### Recommended actions
1. Fix R3-1 (backup pruning). This is the only data-loss item.
2. Fix R3-2 (per-attempt timeout, forget on failure, no retry on refusal).
3. R3-4 as time allows.
4. M5 (TrustReset confirm API) stays deferred as agreed. R3-1 makes it more relevant, because a key flake can leave the user at a trust reset with no way to confirm it.

### Metrics
- Test, clippy, fmt and Swift results are the coordinator's claims and were not reproduced by me.
- No source file over 200 lines was reported.

### Unresolved questions
1. Does `on_offer` dedupe a repeated `item_id` before applying to the clipboard? (R3-2 point 3.)
2. Is the wrong-key move-aside meant to happen on a transient Keychain read failure, or should the app refuse to start in that case?

**Status:** DONE_WITH_CONCERNS
**Summary:** Score 9.3/10, 0 critical. R2-1, R2-2 and the Lows are fixed and match the code. Two round-4 changes add problems: `prune_backups` can delete the only copy of the real history after two wrong-key starts (R3-1), and the new `send_one` ack wait stalls 5 s per publish on a half-dead cached connection and never forgets it (R3-2).
**Concerns:**
- Below 9.5, so no auto-approve. R3-1 and R3-2 should be fixed first.
- I did not run any tests, clippy, e2e or Swift.
- The `on_offer` dedupe behaviour is unchecked.
- M5 stays deferred as agreed.

---

## Code Review Round 4

**Score: 9.4/10. Critical issues: 0.** This is still below the 9.5 auto-approve bar. R3-1, R3-2 and R3-4 are fixed in substance and the dedupe claim is confirmed. The new recovery code has one real flaw in its "never delete a backup that holds data" rule, and the new `HistoryKeyMismatch` path leaves the app with no way out. I did not run any tests or the e2e script; everything below is from reading code.

### Verification of the round-5 claims

| Item | Verdict | Evidence |
|---|---|---|
| R3-1: keycheck | Works as described | `history/recovery.rs` stores `blake3::keyed_hash(key, const)` in `history.keycheck`. `key_verdict` compares in constant time and returns Same, Mismatch or Unknown. The tag is written only after a successful open (`record_key`). It reveals nothing useful about a random 256-bit key. |
| R3-1: no move-aside on a wrong key | Works as described | In `history/db.rs`, an `unreadable` error with verdict Mismatch returns `CoreError::HistoryKeyMismatch`. Nothing is moved, no fresh DB is created and no `HistoryReset` is emitted. A Same or Unknown verdict moves the files aside, emits `HistoryReset` and opens a fresh DB. With the right key again, the original DB opens normally. The mapping to `FsyncError::HistoryKeyMismatch` exists in the service and the FFI. |
| R3-1: pruning "keeps 3 and never deletes one that holds data" | **Not fully true** (R4-1) | |
| R3-2: `send_one` / `attempt` | Fixed | The first attempt's limit is `min(send_timeout, 2 s)`. It applies separately to connect and to send plus ack. On a Net error or timeout, the connection is forgotten and closed (`dialer.forget`, `conn.close`), and one retry on a fresh connection follows. A peer refusal (`stopped()` returns `Some`) becomes `CoreError::Invalid`, which is not retried, and the connection is kept. This covers the stale-connection stall and the refusal amplification from my last review. |
| R3-2: dedupe | Confirmed | `clip/receive.rs::on_offer` calls `first_time(item_id)` and returns `Ok(())` for a duplicate. The check runs before the echo mark, the history record and the event. A retried frame is therefore harmless. The seen-set is in memory (`SEEN_CAP`), so a receiver restart between attempts could process a duplicate once. That is acceptable. |
| R3-4: size cap | Fixed | `OLDEST_WITH_OWN_BLOB` picks the oldest unpinned item that has a present rep whose hash no other item or file references. The loop stops when there are none. Test `size_cap_skips_items_that_free_nothing` exists. |
| e2e | Fine | `BUILD` is made absolute (`mkdir -p` then `cd && pwd`). The latency measurement and the `via` check are as described. |
| `error_mapping_roundtrip.rs` | Passes but is incomplete (R4-4) | |
| `cli_surface.rs` | Fine | Black-box tests of help output and clap usage errors (exit 2), with no network access. |

### Medium

**R4-1. `prune_backups` compares old backups against the live DB, not an empty DB.** (`history/recovery.rs`, called unconditionally from `History::open` in `db.rs`)

`footprint(fresh)` is the size of the current `history.db` plus its WAL. The comment says "the current, just-opened file", i.e. a freshly created empty DB. But `prune_backups` runs on every open, and on a normal start the live DB is large. Any backup older than the newest three whose footprint is not above the live DB's is then treated as "no real history" and deleted. A backup holding real data can therefore be removed whenever the live DB is bigger.

This needs four or more backups, which now arise only from true corruption or from the Unknown verdict (no keycheck file, so a pre-tag DB). It is rare. But it contradicts the explicit claim, and it is the same kind of data-loss path I raised last round.

Fix, any one of these:
- Call `prune_backups` only in the move-aside branch, where the new DB really is empty.
- Compare against a fixed baseline (the footprint of a freshly migrated DB).
- Never delete a backup that has any row in `items`.

**R4-2. `HistoryKeyMismatch` leaves the app stuck, with no recovery path.** The 08b report notes this as deferred, but it matters more now.
- `FsyncService::new` fails with `HistoryKeyMismatch`, and nothing in the service or FFI can abandon the old history or accept a new key.
- The usual real cause is a lost or replaced Keychain item. Previously that case reset the history automatically. Now it is a permanent startup failure until someone deletes the files by hand.
- The history DB also holds the pinned trust, so this ties into the deferred M5 (TrustReset confirm).
- The wrong-key detection itself is correct and prevents silent data loss. The missing piece is a deliberate, explicit user action.
- Fix: add a small explicit call, for example `FsyncService::reset_history(data_dir)` (a free function, because `new` fails). It would move the files aside and clear the keycheck. The app would call it after the user confirms. Add a Swift test for it.
- Needed before the macOS app work starts, not before this review passes.

### Low

**R4-3. `attempt` limits are per step, not per attempt.** Connect and send each get `limit`, and the retry gets `send_timeout` (5 s) per step.
- Worst case for a dead peer: 2 s + 2 s for the first attempt, then 5 s + 5 s for the retry, so about 14 s against a `send_timeout` of 5 s.
- Typical case, an offline peer where the dial hangs: 2 s + 5 s = 7 s, up from 5 s before.
- `publish` uses `join_all`, so this is the bound on the whole call.
- Fix: one deadline per attempt that covers both steps (`timeout(limit, async { connect; send })`), and document the total.

**R4-4. The error roundtrip test omits the two variants added in rounds 4 and 5.** `error_mapping_roundtrip.rs::all_variants` lists 10 variants and lacks `NotFound` and `HistoryKeyMismatch`. The test named "every variant roundtrips" therefore does not cover the newest ones. The match arms would still fail to compile if one were missing, but message text drift would go unnoticed. Add both, or build the list so that a new variant forces an update.

**R4-5. A force-closed connection also aborts other in-flight sends.** `conn.close()` on a cached connection that several tasks share (two publishes at once) aborts the others. They then take the retry path, and the receiver dedupes the result. This is a cost, not a bug.

**R4-6. The size-cap loop can still drop an item that frees nothing.** The SQL excludes items whose content is shared with another item. It does not exclude blobs protected by a live outbox entry (`gc` skips them). Such an item is deleted from history and frees 0 bytes. The loop still ends, because each pass deletes one item. Only live outbox entries are affected, so the amount is small.

**R4-7. The stream-cap test (about 10 s) was skipped as YAGNI.** Acceptable.

### Positives
- The new recovery design is sound: it uses a keyed tag to tell "wrong key" from "damaged file", and it was clearly written to avoid destroying data on a flaky Keychain read.
- The `attempt` function has clear semantics and the right error classification (retryable Net errors, non-retryable refusal).
- Retried frames are safe because the receiver dedupes by `item_id`.
- Source file sizes stay under 200 lines (`retention.rs` is 196, `recovery.rs` 113).

### Recommended actions
1. R4-1: prune only after a move-aside, or compare to a fixed empty-DB baseline.
2. R4-2: add an explicit reset-history call before building the app.
3. R4-3 and R4-4 (small).
4. M5 stays deferred as agreed.

### Metrics
- Test, clippy, fmt and Swift results are the coordinator's claims and were not reproduced by me.

### Unresolved questions
1. When will the app learn about `HistoryKeyMismatch`, and who confirms the reset?
2. Should the Unknown verdict (DB without a keycheck file) keep the old auto-reset behaviour, given that the keycheck now exists for all new DBs?

**Status:** DONE_WITH_CONCERNS
**Summary:** Score 9.4/10, 0 critical. R3-1 (wrong key vs corruption), R3-2 (single bounded retry, refusals not retried, dedupe confirmed) and R3-4 are fixed. R4-1 (pruning compares against the live DB size, so old data backups can be deleted) and R4-2 (no way out of `HistoryKeyMismatch`) keep the score below 9.5.
**Concerns:**
- R4-1 is a data-loss path under a rare condition, and a quick fix (prune only after a move-aside) would clear it.
- R4-2 is a contract gap that must be closed before the macOS app uses this.
- I ran no tests, clippy, e2e or Swift, so I rely on the coordinator's gate results.
- M5 is deferred as agreed.

---

## Code Review Round 5

Scope: the round-6 fixes in `core/fsync-core`, `core/fsync-ffi`, `core/fsync-cli`, and the test helper `backend/tests/common/pg.rs`. All verification is by reading code. I ran no builds or tests. The gate results (fmt, clippy, workspace tests, swift test, e2e) are the coordinator's claims, not reproduced. The final-gates agent was still running when I wrote this.

### Verification of round-4 items

| Item | Verdict | Evidence |
|---|---|---|
| R4-1 prune only after a move-aside | Fixed | `history/db.rs` calls `recovery::prune_backups(dir, &path)` only in the move-aside branch, right after `open_writer` creates the fresh empty DB. The normal open path only calls `record_key`. The doc comment says never to call it with a live DB. Residual: see Low L1. |
| R4-2 `reset_history` in core, service, FFI and CLI | Implemented. Re-pair path unverified (M1) | Core: `history/recovery.rs:78`, exported at `history/mod.rs:13`. Service: `service/mod.rs:71`. FFI: `fsync-ffi/src/lib.rs:37`. CLI: `cmd_account.rs:106`. Without `--yes` it bails before touching anything. It uses `g.data.join("core")`, the same directory the service opens. Tests: `tests/history_db.rs:287-307` and `service_facade.rs:181`. I did not read the CLI argument wiring or the Swift test closely. |
| R4-3 one deadline per attempt | Fixed | `clip/send.rs` `attempt()` sets `deadline = Instant::now() + limit` and uses `timeout_at` for both connect and send+ack. The first attempt uses `min(send_timeout, 2 s)` and the retry uses `send_timeout`. A Net error or timeout calls `dialer.forget` and `conn.close`. A refusal (`stopped()` returning Some) becomes `CoreError::Invalid` and is not retried. The worst case per peer is about 7 s. |
| R4-4 exhaustive error test | Fixed | `fsync-ffi/tests/error_mapping_roundtrip.rs` has an exhaustive `variant_index` match with no wildcard. It also checks `VARIANT_COUNT = 12` against distinct samples, so a new variant breaks the build and a missing sample fails the test. `NotFound` and `HistoryKeyMismatch` are now covered. The roundtrip and message-equality tests both use `all_variants()`. |
| R4-6 size cap skips outbox-protected blobs | Fixed | `history/queries.rs:171` `load_guarded` mirrors the outbox-protected hashes into `temp.guarded`. `retention.rs:114` calls it before the size loop. `OLDEST_WITH_OWN_BLOB` adds `AND r.hash NOT IN (SELECT hash FROM temp.guarded)`. The temp table is per connection and is cleared and refilled each run, so there is no stale state. I did not read the test `size_cap_leaves_items_whose_blob_the_outbox_protects`. |
| `backend/tests/common/pg.rs` retry | Acceptable | `host_port` retries `get_host_port_ipv4` 25 times at 200 ms (about 5 s) and panics with the last error. It is test-only helper code. See Low L3. |

### Critical
None.

### High
None.

### Medium

**M1. Recovery after `reset_history` may dead-end at re-pairing (not verified end to end).**
- After a reset, `trust.list()` is None, so `ensure_account` returns `AccountState::NeedsPairing`.
- The backend still lists this device. I did not trace any code that removes it.
- The re-pair path builds the new device list with `DeviceList::next_add` (`fsync-proto/src/device_list.rs`, around line 101). From reading it, it returns `ProtoError::DuplicateDevice` when the node id is already in the list. It is used by `pair/initiator_steps.rs` `publish_new_list`.
- So re-pairing the same node id from an existing device will probably fail, unless the initiator skips the publish for an already-listed node or the user first removes the device from the account.
- Nothing tests the sequence reset -> pair again. I did not check the initiator for special handling of an already-listed node.
- The docs say "re-pair (or trust reset)", and trust reset is the deferred M5, which I am not counting against the score.
- Fix: add one integration test that runs `reset_history`, then opens with the new key, then pairs again using the same node id. If it fails with `DuplicateDevice`, either make `next_add` idempotent for an identical entry on the re-pair path, or document and expose "remove this device from another device first".

### Low
- **L1. Tiny-backup edge case.** A data backup no larger than an empty DB can still be pruned beyond the newest three. The dev already noted this. It only applies after a move-aside, so I accept it.
- **L2. Reset backups are never pruned.** `history.corrupt-*` files made by `reset_history` are not pruned. They only grow on user-initiated resets, so the growth is negligible.
- **L3. `pg.rs` retry is not selective.** It retries on any error, not just `PortNotExposed`, and it panics after the budget. That is fine for a test helper, but matching on the specific error would avoid hiding other failures for 5 s.
- **L4. Formatting.** One doc comment line in `send_one` (`clip/send.rs`) is overlong.

### Positive observations
- R4-1, R4-3 and R4-4 are fixed with the guard at the right place. The prune call sits only in the branch where its input is trustworthy, and the doc comment says so.
- The exhaustive match plus a count check is a good compile-time guard against drift between the service and FFI error types.
- `reset_history` treats a missing directory as Ok, so it can be re-run. The CLI refuses without `--yes` before touching anything, and the message names the pinned-trust loss.
- The retry classification in `send.rs` is correct: transport errors retry, a refusal does not.
- The retention guard reuses the existing `protected()` source rather than adding a second one, which keeps DRY.

### Recommended actions
1. Resolve M1 with the reset -> re-pair test, or tell me how the initiator handles an already-listed node. This is the only thing between this pass and a clean auto-approve.
2. Optional: match on the specific error in `pg.rs`, and wrap the long doc line.
3. Keep M5 (the TrustReset confirm API) on the roadmap. Nothing in the app confirms a reset yet.

### Metrics (not reproduced)
- Gates, as claimed by the coordinator: fmt clean, clippy `-D warnings` clean, `cargo test --workspace` 56 ok lines, swift test 6 pass, e2e PASS (direct clip 0.03 s, relay 0.03 s).
- Critical: 0. High: 0. Medium: 1. Low: 4.

### Unresolved questions
- Does the pairing initiator tolerate a node id that is already in the device list after `reset_history`?
- Who confirms the reset in the app? There is no UI or API path for it until M5 lands.

### Score: 9.5/10, 0 critical

The score holds only if M1 turns out benign. If the re-pair test fails with `DuplicateDevice`, the documented recovery path does not work and I would drop it to about 9.2.

**Status:** DONE_WITH_CONCERNS
**Summary:** Round-6 items R4-1, R4-3, R4-4 and R4-6 are fixed. R4-2 is implemented in core, service, FFI and CLI, but I could not confirm that re-pairing works afterwards. Score 9.5/10, 0 critical issues, 0 high.
**Concerns:** M1: reset -> re-pair with the same node id may fail with `DuplicateDevice`; this came from reading `next_add` and `ensure_account`, not from running it, and no test covers it. I did not read the R4-6 test, the CLI argument wiring, the Swift test or the docs changes. No builds or tests were run by me. M5 is deferred as agreed.

---

## Code Review Round 6

M1 is fixed. I confirmed it by reading the code. I ran no builds or tests, and I did not check the "Review round 7 fixes" notes in the dev report.

**Security: no bypass.**
- `listed_head` (`initiator.rs:157`) is reached only after every gate has passed:
  - the MAC confirm on both sides
  - device-name validation
  - the local `ConfirmRequest` accept (SAS)
  - the new device's `PairFrame::Accept`
- The branch only skips the publish. It does not skip any consent or authentication step.
- `listed_head` hands out `trust.head()` only if the pinned list already contains `n_id`. A device that is not on the list still goes through `publish_new_list`.
- The branch adds no new authority. Its only effect is to give a node already on the account's list its head again. The peer was authenticated by the pairing secret and approved by the user.

**Correctness: the new device's checks still apply.**
- The new device still runs `pin_paired_head` (`trust/ingest.rs:50`). It refuses if a list is already pinned and verifies the signature. It also checks that the signer is the peer it paired with, that the account matches, and that the list contains both itself and the peer.
- The head comes from E's own pinned state. Nothing from the network is substituted.
- The branch does not call `trust.ingest`, which is correct because E's head is already pinned.

**Tests.**
- `pairing_repair_after_reset.rs` covers the happy path. The head seq stays at 1, the backend still holds 2 entries, both sides trust each other, and SAS matches.
- It also covers the case where one side declines: both sides fail and the reset device pins nothing.
- I did not read `tests/common/pairing.rs` (the `pair_on` helper) in detail. The tests depend on it.

**Low.**
- A stale E head is possible if E's pinned list is behind the backend. That is the same behaviour as before, and the next sync heals it.
- The branch is not tested with a malicious E, but `pin_paired_head` covers that.

Score: 9.6/10, 0 critical, 0 high, 0 medium. M5 (TrustReset confirm API) stays deferred as agreed.

**Status:** DONE
**Summary:** The listed-head branch runs only after mutual SAS acceptance and the MAC confirm. It skips only the publish, and the new device still validates the head fully. Score 9.6/10, 0 critical.
**Concerns:** None blocking. This is based on reading the code only. I did not read `tests/common/pairing.rs` in full. M5 remains deferred.

**Gate: auto-approved (score 9.6 ≥ 9.5, 0 critical).**
