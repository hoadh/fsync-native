# Delayed W->M clip (92 s) + `clip watch --count 1` timeout

Diagnosis only. No source edits. Branch core/hardening (worktree) checked: its diff vs 85f8968 does not touch clip/, net/, events.rs, or CLI, so same conclusion on both.

## Executive summary
- **No 92 s delivery delay happened. No product bug in the delay path.** Observed symptom = test-harness artifact: **two fsync-cli processes shared one Mac `--data` dir (same node id)**, so offers went to whichever process the sender reached. Tester compared timestamps of two different clip items.
- Real product gap exposed: **nothing prevents two engines on one data dir** (same identity.key, same history.db). Silent misrouting, "delivered" still reported. Medium.

## Evidence (files in /private/tmp/claude-501/-Users-alex-Code-fsync-native/pairtest/)
1. Different items. `clip-direct/put.out` item `01a13add-758d-...`, published 21:42:34 (uuid v7 ms 1791988954509). `clip-direct/watch.out` clip item `01a13ade-d876-...` text `hello-repro-w2m` (uuid ms 1791989045366 = 21:44:05.4), `ts_ms` 1791989046633. That item was minted 21:44:05.4, received 21:44:06.6 = **1.27 s**, normal latency. "92 s" = repro item's receive time minus first item's publish time.
2. The "timeout" belongs to the other watcher. `clip-direct/watch.err` is empty (watch #1 exited 0 after `--count 1`). `error: timed out` is `clip-repro/watch.err`, mtime 21:46:04 = launch 21:44:04.674 (sample-watch.txt) + 120 s. Watch #2 got nothing because the repro put went to watch #1 (older, same node id). `sample-watch.txt` shows pid 35479 idle in `Ctx::wait_event -> recv_timeout`, not hung. So the "watch printed but did not exit" question is moot: no process did that.
3. Where did the first clip (21:42:34, "delivered" to node 3185...) go? Not into watch #1 (its out has only the repro clip). An orphan Mac process with the same identity was alive: `pair-mac.pid` 33094 (21:38:27), `pair-mac.out` offer expires_ms 1791989010964 = 21:43:30.96, `pair-mac.err` "pairing offer expired" mtime 21:43:30. Abandoned Mac `pair offer` (Windows's offer was used instead, join 21:40:57) kept a full engine up on the same data dir for 21:38:27-21:43:30, covering the 21:42:34 put. Its ClipHandler took the offer, wrote shared history.db (matches "Mac history recorded the item"), emitted `RemoteClip` to a channel nobody read. Inference (process gone, no log), but fits every timestamp.
4. Local repro (scratchpad/dup-identity-repro.sh, own PG container `fsync-dupid-pg-*`, removed; `mystifying_euclid` untouched). A + B paired; two `clip watch --count 1` on B's same data dir (w1 first, w2 1.5 s later); A puts 3 clips. Result: put1 -> w1 only; put2 -> w2 only; each put "published delivered". Each offer goes to exactly one process, no broadcast. Reproduces both symptoms: a watcher that sees nothing and times out, and a clip seen by the "wrong" watcher.

## Hypotheses eliminated
1. **Lazy text fetch depends on origin liveness: eliminated.** Text <= `MAX_INLINE_TOTAL` 256 KiB (fsync-proto clip.rs:14) rides inline in the offer (clip/offer.rs `build`, limits.rs `choose_inline`). `Engine::fetch_rep` returns inline bytes with no network (engine/api.rs:75-77); CLI watch calls it (cmd_clip.rs:63) but no fetch occurs for 15-22 byte text. Sender also waits for transport ack before returning (clip/send.rs:134 `tx.stopped()`), so exiting after `put` does not cut the frame. Only >256 KiB reps would depend on origin; not this case.
2. **Event drop / lag: eliminated.** events.rs:145-149 only drops when the 256-slot channel is full; one clip cannot fill it. Pump (service/events.rs:14-18) is a straight loop. Count logic cmd_clip.rs:54-77 is correct (exit 0 at seen==count; watch #1 did exit 0).
3. **Stale direct addr / hole-punch ~90 s: eliminated.** Send path bounded: `send_one` = 2 s cached attempt + `send_timeout` (7 s) fresh attempt (send.rs:97-104), dial limited by `DIAL_TIMEOUT` 10 s (net/dialer.rs:21). Max ~17 s, cannot produce 92 s. put.out also shows attempts=1, settle_ms 703. LAN 192.168.1.x unreachable did not matter: dial is by node id, addresses via discovery (dialer.rs:62), all runs fell back to tailnet/relay. Cannot rule out minor first-dial latency but evidence shows 1.2 s.

## Root cause
Test procedure: concurrent processes (orphan `pair offer` pid 33094; watch #1 and #2) on one `--data` dir. Core permits it: no data-dir lock anywhere (grep flock/try_lock/lock_exclusive in core/: none; `fs4` is already a dependency, used only in fs_util.rs:72). `FsyncService::start` (service/lifecycle.rs:72) and `Identity::load_or_create` (identity.rs:55) accept a second user of the same identity/history.

## Fix proposal
1. In core, take an exclusive advisory lock (`fs4::fs_std::FileExt::try_lock_exclusive`) on `<data_dir>/engine.lock` at `FsyncService::start` (or `FsyncService::new`), held for the service lifetime, released in `stop`/Drop. On failure return a typed error (e.g. `FsyncError::DataDirInUse`) with message "data dir in use by another fsync process". Apps (one engine per data dir) unaffected. Commands that must not start the engine (`status`-like) are unaffected.
2. Test (would have caught it): `core/fsync-core/tests/data_dir_lock.rs`: create two `FsyncService` on the same `data_dir`, `start()` first ok, second must return `DataDirInUse`; after `stop()` of the first, second start succeeds. CLI-level: spawn two `fsync-cli clip watch` on one `--data`, second must exit non-zero within seconds with that message.
3. Harness/test hygiene: tester scripts must kill the `pair offer` process after pairing completes and use `pgrep -f "fsync-cli.*--data $MAC_DATA"` to assert one process per data dir before each clip run. Compare clip timestamps by `item_id`, not by file mtime/order (add item_id match in clip-w2m.sh).
4. Optional observability: CLI tracing subscriber (RUST_LOG) so `engine event dropped` shows; tester rec #1 stands but is not the cause here.

## Unresolved questions
- Orphan process 33094 identity/data dir not directly proven (no ps snapshot). Check shell history for `pair-mac` start command to confirm `--data data-mac`.
- Path used by Windows (tailnet direct vs relay) unverifiable (label "Lan" for any direct IP, tester noted).
- Relay log race in my repro script (grep before file existed); test still passed over backend discovery, relay use not asserted. Irrelevant to the conclusion.
- Windows not used; no Windows-side logs of the 21:42:34 dial (would show the connection reached the orphan).

**Status:** DONE
**Summary:** The 92 s delay and watch timeout were a misreading: two same-identity Mac processes shared one data dir, so offers landed in the wrong process (reproduced locally; item_id/timestamps prove the "late" clip was a different item delivered in 1.27 s). All three hypotheses eliminated with code and data. Real gap: no data-dir single-instance lock; fix and test proposed.
**Concerns/Blockers:** Orphan-process attribution for the first clip is inferred from pid/err mtimes, not directly observed. No code edits made; repro script in scratchpad only.
