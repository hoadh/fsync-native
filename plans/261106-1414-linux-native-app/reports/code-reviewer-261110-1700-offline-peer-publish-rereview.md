# Re-review: offline peer publish delay (rework 1811d75 + 73277c4 + 90d3b47)

Net diff `ac0f4eb..HEAD -- core apps docs`. Score 9.0/10. Critical 0, high 0, medium 3, low 6. NOT approved (bar 9.5): core is correct; gaps are consumer feedback and test proof.

Previous review: H1 (push skipped), H2 (returning peer loses clips), M1-M4 all resolved by the redesign (skip map gone, push bypasses deadline + lanes, returning-peer test).

## Verified correct (core)
- Lanes (`clip/peer_send_lanes.rs`): seq taken synchronously in `start_send` before spawn; check `latest == seq` after acquiring the lock. Correct regardless of lock/spawn order: a newer ticket that wins the lock first sends, the older one then sees `latest != seq` and skips. Latest clip IS delivered eventually: N running, N+1 queued -> N+1 sends after N; N+1 and N+2 queued -> N+1 skipped, N+2 sent. No drop of the newest.
- No wedge: guard is a tokio `MutexGuard` bound in the match arm; dropped on completion, on abort (future dropped) and on panic unwind (tokio mutex does not poison). `send_one` is bounded (2 s + `send_timeout`), so a lane is held at most ~7 s.
- Guard released before `tx.send`, so a fast peer never looks `busy` to the next serialized publish.
- JoinSet reaped via `try_join_next` on each spawn; residue = finished-since-last-spawn only. Tasks hold `Arc<Shared>` not `Arc<Engine>`, so `wait_unique` in `stop` is unaffected; no permanent cycle (tasks end within ~7 s).
- Service runtime is multi-thread, so background tasks progress between blocking FFI calls.
- Close: `background.close()` takes the set first (new spawns refused -> `rx` error -> `Closed` -> `failed`), then abort_all + join, then `node.close()`.
- Abort mid-stream: receiver reads a length-prefixed frame with `read_to_end`; a truncated/reset stream fails decode -> rejected, never applied. Half-written clip cannot land.
- Outbox: task unoffers on every Err incl. superseded; aborted tasks skip unoffer only at close (moot).
- fsync-cli: `publish_clip_and_wait` keeps one-shot semantics; no regression.
- `rate_limited_offers_are_failed_not_refused` change is legitimate: concurrent publishes to one peer are now serialized by design; sequential burst still exercises receiver rate limit (asserts `failed > 0`, `pending` empty). Rate-limit feedback path unchanged.

## Medium
- M1: FFI consumers lose offline feedback and Windows can misreport. `pending` is dropped from the FFI record, and a dead peer always exceeds 1.5 s, so it is never `failed` any more:
  - macOS `ClipboardSync.swift:159`: "Some devices couldn't be reached." never shows for an offline device (was shown before).
  - Windows `ClipboardService.Outgoing.cs:77-78`: `LastError` "N device(s) did not get it" gone for offline peers; and a live-but-slow peer (cold relay dial > 1.5 s, the only other device) yields "Copied, but no other device is connected", which is false. Accepted in the implementer report, but the contract change is silent for Swift/C#. Fix options: add `pending` to FFI (binding regen) or document in ffi-contract that `delivered + failed + refused` may be a strict subset of targets and apps must not infer "not connected" from `delivered == 0`.
- M2: Linux status "Sending to N device(s)..." is never resolved: late outcome is only debug-logged, so with one offline device the line stays "Sending..." until the next clip (previously "Not sent: no other device is reachable."). No event for late results. Acceptable if the tray shows connectivity elsewhere; state it.
- M3: No integration test proves latest-wins against a slow-but-live peer (N in flight, N+1 and N+2 queued -> N+1 never arrives, N+2 arrives after N). Only unit tests on the lane primitive. And `returning_peer_gets_the_next_clip` asserts `got.last() == next` after a loop that stops exactly when `next` arrives: tautological, it cannot detect `away` arriving after `next` (claim "never an older one after it" not proven). Fix: drain events for a short window after `next` and assert nothing older follows; add a slow-receiver test (e.g. receiver-side delay hook or a peer behind a delayed accept).

## Low
- L1: Concurrent publishes (not used by apps) on a multi-thread runtime can interleave tickets per peer (A gets newer seq on p1, B on p2): per-peer "latest" may differ from item_id order. Document "publishes must be serialized for ordering".
- L2: Queued tasks behind a dead peer's 7 s attempt each hold their `Arc<[u8]>` frame (up to ~256 KiB inline) until they get the lock and skip; a copy burst holds N frames for up to 7 s. Bounded, but a single "latest slot" per lane would avoid it.
- L3: Task panics are swallowed by `try_join_next`/`join_next` (no log) and skip unoffer; add `if let Err(e) = r && e.is_panic()` log.
- L4: Wall-clock bounds: `QUICK` 2 s vs 1.5 s deadline (0.5 s margin) and `< 1 s` for busy-lane publishes; rate-limit test needs 60 sequential publishes in < ~2 s to hit the limit. Passed 5/5 here under parallel load; watch on slow CI.
- L5: `closing_the_engine_cancels_a_pending_send` allows 10 s and its post-close check is conditional (`if let Ok(Sent)`); abort is proven only by the unit test. Aborted attempts skip `dialer.forget`/`conn.close`; `Endpoint::close` then waits ~3 s (implementer measured) for the dead peer's connection. With `TEARDOWN_STOP_TIMEOUT` 3 s (1 s graceful budget) an app quit within ~7 s of a copy with an offline peer likely hits the documented "engine dropped" worst case (not verified whether this differs from before the change). Pending clips are lost at quit: acceptable (peer was slow/offline; same as before for failures).
- L6: Docs: protocol-spec/system-architecture still say "5 s per-peer timeout" (really 2 s + 5 s retry; pre-existing). Otherwise ffi-contract, protocol-spec step 3, system-architecture step 3 and changelog match the code. `PublishOpts` gained a pub field (`wait_all`): struct-literal users break (all in-repo updated).

## Tests run
- `cargo test -p fsync-core --locked --test clip_offline_peer_publish`: 4/4 pass (4.6 s), 5 repeat runs + `clip_refusal_feedback` 5 runs, all pass (concurrent with workspace build).
- Unit: `peer_send_lanes` 2/2, `background_sends` 1/1 pass.
- `cargo test --workspace --locked --no-fail-fast`: 375 passed, 0 failed, 3 ignored. An earlier run, made while other test loops were running, failed `file_read_ahead::serial_small_reads_overlap_round_trips` once. It is a CPU-timing benchmark that this diff does not touch, and it passed 3 out of 3 times when run alone. The failure is unrelated to this change.
- apps/linux Wayland/X11 backend tests not run (per instruction).

## To reach 9.5
M1 (decide + document contract, or expose `pending`), M3 (real latest-wins test + fix tautology). M2 at least documented.

## Unresolved questions
- How often does a cold relay dial exceed 1.5 s in practice? If common, Windows' "no other device is connected" misreport is frequent.
- Should a late background outcome emit an event so apps can update status (Linux "Sending..." line, Windows/macOS failure hint)?
- Is losing the macOS/Windows offline-device hint an accepted product decision?
