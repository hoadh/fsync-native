# Track K core hardening report

Worktree `/Users/alex/Code/fsync-native/.claude/worktrees/agent-a60263367f2129522`, branch `track-k/core-hardening`, base 68d7ae6 (task text said 07dc9b6; plan says 68d7ae6). Not pushed, not merged.
This file is untracked in the worktree (the harness refused writes to the shared-checkout path); do not commit it.

Commits: cd3a7cf (K1), 4d50e02 (K2), 038ede0 (K3 core), 6f13d0a (K3 CLI).
Touched only `core/**` + `Cargo.lock` (one line: fsync-cli -> tracing-subscriber). No apps/**, tests/e2e/**, plans. `git diff 68d7ae6 -- core/fsync-ffi` empty (no FFI/UDL change).

## K1 fetch retry on dead cached conn
- Test `core/fsync-core/tests/fetch_after_peer_restart.rs`: B warms cached file/1 conn, A crashes (runtime dropped, no CONNECTION_CLOSE), restarts same identity on new port, offers different content, B fetches.
- RED on unfixed code: `fetch hung on the dead cached connection: Elapsed(())`, FAILED after 25.37 s.
- GREEN on fix: 10/10 runs, about 2.5 s each (limit 10 s). dial_after_restart, file_transfer, clip_protocol still pass.
- Fix: `Dialer::connect_reporting_reuse` + `abandon`; `fetch_client` handshake on a reused conn gets 2 s (`FIRST_ATTEMPT`); on Net error/timeout: forget + close + redial once with full STALL. Protocol answers (Denied/NotFound/Changed/Integrity) never retried.
- KEY FINDING: forget + close + redial alone was NOT enough. Dead INBOUND conns from the crashed peer (e.g. its clip/1 conn to us) keep iroh routing to the dead path; redial timed out for 45+ s. Closing them fixed it (2.85 s). Added `Gate::close_stale_inbound(peer, before)`, called from `abandon`. Trade-off: also closes a fresh inbound conn from the restarted peer and any in-flight inbound pull from it (they redial/resume). Without any inbound conn the dead conn is detected fast, no hang.

## K2 engine.lock
- M1/M2 already closed on main by 7817300, 7e92f3d, 7b9cd0f (MVP phase-08 line 90 is stale).
- New `service/lingering_hold.rs` tracks our own holder deadline; `lock_data_dir` on WouldBlock logs `warn!` "previous engine still releasing engine.lock, up to Ns left; retry then" and still returns `EngineAlreadyRunning` (FFI unchanged). Optional wait not implemented.
- Test `start_during_own_lingering_hold_is_refused_then_works_after_release` in `stop_leak_tests.rs`; stop_leak 3/3 stable.

## K3 path tier + tracing
- Core: `net/path_tier.rs` (new; `lan_first_path_selector.rs` untouched at 198 lines): `pub enum PathTier {Lan, Direct, Relay}` (core-only, exported from `fsync_core`, not FFI), `tier_of_addr`, `connection_tier`; `Gate::connections_of`; `Node::peer_path_tier` (best = min over live conns); `FsyncService::peer_path_tier(&str) -> Option<PathTier>` in `service/path_tier.rs`.
- Deviation 1: loopback counts as `Lan` (`LocalSubnets::contains` excludes loopback, so the selector rule alone says `direct` for two nodes on one host; plan requires `lan` on loopback).
- Deviation 2: in `fetched` the key `path` already holds the saved file path, so the tier there is `path_tier`. `sent` and `devices --json` use `path` as planned. `via` unchanged.
- Tier is remembered while the transfer runs (peer may close right after Done): first attempt sampling only at Done gave null on fetched in p0.
- Unit tests: IPv4, IPv6, loopback (v4/v6/mapped), relay addr, ordering. Integration `tests/path_tier.rs`: loopback peers -> lan both directions, none without conn; relay-only on in-process relay -> relay.
- CLI: `tracing-subscriber` (env-filter, fmt) installed only if `RUST_LOG` set, stderr, ANSI only on tty. Verified: `RUST_LOG=debug` prints traces on stderr; unset prints nothing extra; stdout identical.
- p0 output (kept logs): direct round `sent path=lan`, `fetched path_tier=lan`; relay-only round `sent path=relay`, `fetched path_tier=relay`; devices JSON has `path` (null when no live conn, as in the post-run listing).

## Gates
- `cargo fmt --all -- --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok
- `cargo test --workspace`: exit 0, 74 result lines, 0 failed
- `tests/e2e/p0-exit-demo.sh` MIB=16: PASS (twice; before and after the tier-memory change)
- `scripts/build-xcframework.sh`: ok (about 1.5 min); `swift test` in apps/macos/FsyncMac: 231 executed, 18 skipped, 0 failures. Build output gitignored (tree clean).
- New modules 110/49/15 lines; no touched file over 200.

## Unresolved questions
1. Is closing ALL older inbound conns from the peer on a fetch retry acceptable, or narrower (e.g. only conns on the dead path)? Needs iroh path introspection; not done.
2. Other dial paths (clip, pairing re-dial) rely on outbound-only `close_stale`; the same dead-inbound routing block may delay them 45+ s after a peer crash. Separate fix?
3. K2 optional bounded wait (min(remaining, 2 s)) skipped; wanted?
4. `fetched` key `path_tier` vs `path`: ok for Track N / macOS integration consumers?
5. Base was 68d7ae6, not 07dc9b6 as in the task text; confirm intended.
6. Windows/dotnet run not done (no FFI change).

---
# Round 2: review fixes + items A and B

Commits: 6fed9d9 (review fixes), 3ab3888 (A: backend-down start). Gates after both: fmt ok, clippy -D warnings ok, `cargo test --workspace` rc 0 / 0 failed, p0 MIB=16 PASS.

## Review fixes (6fed9d9) - supersedes the K1 "KEY FINDING" and unresolved Q1/Q2 above
1. `Dialer::abandon(peer, alpn, conn, rx_before)`: drops the cache entry only if the cached conn has the same `stable_id` (a concurrent fresh dial is never evicted). Closes the conn only if it received zero UDP bytes since the caller's mark; in-flight streams of a healthy shared conn always receive data/acks, so they are not cut off.
2. Inbound cleanup: `close_stale_inbound` removed. On a first-attempt dial stall only: `forget` + `close_stale` (outbound, same ALPN) + `Gate::close_inbound_except_active_pulls` (spares any `file/1` inbound conn that received bytes since a mark taken before the dial). Not called on a first-attempt timeout of the fetch handshake.
   - Correction of my earlier finding: the blocker is not "dead inbound conns" but ANY live connection to the peer. Measured: closing only silent inbound conns, or only fresh ones, both FAILED the regression ("dial timed out"); closing all passed. iroh keeps the stale remote path while any conn to the peer exists. So no narrower trigger/set works without an iroh API to reset remote path state.
   - Residual deviation from "never kill clip/presence": when the redial stalls, the peer's inbound clip/presence conns are closed and reconnect on next use. Pulls with traffic are spared.
3. `FIRST_ATTEMPT` 2 s -> 5 s (slow relay ~0.4 MiB/s). Retry only if `Transfer(Timeout)` or `Net` error with the conn already closed (`is_dead_path`); other errors propagate.
4. DRY: `PathTier` is now a re-export of the selector's `Tier` (made `pub`, one-word change; selector file still 198 lines); `tier_of_addr` = loopback -> Lan, else the selector's `tier_of`.
5. CLI help documents `path` (sent, devices) vs `path_tier` (fetched; `path` there is the saved file).
6. Tests (`tests/fetch_shared_connection.rs`): abandon spares a conn with traffic and a newer cache entry, late abandon does not evict the new conn, silent conn is closed (Dialer level, deterministic; stands in for "concurrent fetch survives", a timing-based serve_delay variant would be flaky); slow (3 s serve_delay) healthy answer on a cached conn is not abandoned. 5/5 stable.
7. `fetch_after_peer_restart` still the same file; passes in 5.7-7.7 s (5 s first attempt + 2 s redial stall; limit 10 s, 25 s outer); RED on unfixed code proven earlier (25.37 s hang).

## A: CLI cannot start without the backend (3ab3888)
- Fatal call: `Inner::ensure_account` (`service/account.rs`): `get_device_list` / `sync_device_list` errors propagated. `start()` does no API call; presence already retries with backoff in the background.
- Fix: on `CoreError::Net|Http` and a device that has a pinned list including itself: warn "backend unreachable, using the cached device list", restart presence, return `Ready`. Other errors, or nothing pinned, still error.
- FFI: same `Inner::ensure_account`, so apps behave the same; no FFI change.
- Test `tests/service_backend_down.rs` (real backend stopped by dropping its runtime): pinned device -> Ready with self in devices; device with nothing pinned still errors.
- Not re-run: the exact netns CLI repro (Linux privileged container); covered at service level.

## B: ~4 s first connect on routed/NAT'd peers: not the dialer
- Measured (in-process relay with no QAD, peers not force-relay): dialing with relay + unreachable direct hints takes 5-14 ms, so `FIRST_ATTEMPT` is never reached.
- Cause: iroh publishes our relay URL (and `online()`) only after its net report finishes; the local relay has no QAD, so that takes ~3.0 s (relay URL known at 3.05 s, `wait_online` 3.05 s). Hello goes out after 300 ms without the relay URL; the relay URL follows at ~3 s, only then can the peer dial via relay. Forced-relay (IP transports off) skips the net report, hence 0.02 s in p0. A production relay with QAD answers in ms.
- No dialer change made (nothing to fix there). Option not done: advertise the configured relay URL in Hello before iroh confirms it (risk: peer dials a relay we are not yet connected to). N4's 1.5 s target needs either QAD on the test relay or that change.

## Unresolved questions (round 2)
1. Is closing the peer's clip/presence inbound conns when a redial stalls acceptable, or should we look for an iroh API to drop stale remote path state?
2. B: add early relay-URL advertising in Hello, or give the test relay QAD (N3 hook)?
3. Round 1 questions 3-6 still open (K2 optional wait, `path_tier` key, base commit, Windows run).

---
# Round 3 (review 9.2 -> fixes)

Commits: 8f2c6e7 (offline fallback fail-closed), 1bad0ac (cleanup escalation + test determinism).
Gates: fmt ok; clippy -D warnings ok; `cargo test --workspace` run twice, rc 0 both; p0 MIB=16 PASS; xcframework built (scripts/build-xcframework.sh) + `swift test` 231 executed / 18 skipped / 0 failures. No FFI change (FsyncError mapping only gained a variant arm inside core).

## 1. Redial-stall cleanup narrowed (1bad0ac)
- Dial escalation, one step per stalled 2 s attempt: stage 1 forget + close our older outbound conns (same ALPN); stage 2 `Gate::close_inbound(IdleOnly)`: closes only idle inbound conns of the peer; stage 3 `AllButPulls`: also closes in-use clip/presence conns, spares moving `file/1` pulls. Then the rest of the 10 s budget for the final dial.
- "In use" = received bytes since the pre-dial mark on ANY ALPN, or no mark (opened after the dial began); sampled at close time. Nothing is closed on a first-attempt timeout.
- MEASURED (debug print in the restart test): at stage 2 B has two inbound clip/1 conns from A: the dead old one (silent, closed) and the fresh one from restarted A (rx advanced, spared). With the reviewer's rule alone the redial still fails ("dial timed out", 5/5 runs, 15.4 s). iroh needs the fresh conn gone too, hence stage 3. So the residual cost remains: in the crash-and-restart case the restarted peer's fresh clip/presence conn is closed after ~6 s of stalled dialing and reconnects on next use. Only a stalled redial triggers it; running pulls are never touched.
- Cached-outbound "received nothing" trigger not needed (stage 1 already drops it).
- `fetch_after_peer_restart`: passes 5/5 at 11.6 s; assertion bound 20 s (outer 25 s timeout unchanged; unfixed code hangs past 25 s, RED proven in round 1, test logic unchanged otherwise). Not re-run on unfixed code this round.
- New test `close_inbound_spares_in_use_connections_until_the_last_stage` (real conns): idle closed at stage 2, in-use non-file conn survives it, closed at stage 3.

## 2. Offline fallback fail-closed (8f2c6e7)
- New `CoreError::Unreachable` produced in `api/check.rs` via `api/reachability.rs::is_unreachable`: timeout, or connect failure whose cause is an io error of kind refused/reset/aborted/not connected/timed out/host or network unreachable/addr not available, or "dns error". TLS/cert failures, bad body, too-large body stay `Http`; HTTP statuses stay `Api`. FsyncError mapping unchanged (Network); revoke retry treats Unreachable like Http.
- Fallback (`service/account.rs::offline_fallback`) accepts only `Unreachable`, needs a pinned list that includes this device, and only if the list was confirmed <= 7 days ago. Else the original error is returned; past the bound logs `warn "stale device list: not confirmed for over 7 days ..."`.
- Last-confirmed time: new file `<data_dir>/list-synced-at` (unix seconds, 0600, atomic) written by `service/list_freshness.rs` after successful `ensure_account` (Created/Ready) and `remove_device` sync. Trust store/SQLite format untouched. No record (older install): the clock starts at the first offline use (stamped then).
- FFI state exposure: skipped (a new field/callback is not free; the warn log carries it). Suggest a later additive `device_list_age_days()` if the apps want a banner.
- Tests: unit (policy: allowed, bound edge, each suspicious error refused; stamp round trip; missing/garbage record), reachability with real local sockets (refused, DNS, timeout -> Unreachable; garbage-TLS server -> Http), service level with real backend stopped (fresh stamp -> Ready, 8-day stamp -> Network error, 6-day -> Ready, nothing pinned -> error). A bad-body/too-large refusal is covered at policy level (those errors are `Http` by construction, existing `security_limits` test pins "too large" -> `Http`), not through a second fake server.
- Limitation: the stamp is not refreshed by background syncs (presence push/pairing), only ensure_account/remove_device; apps call ensure_account at each start, so a long-running device that only synced via push could see an older stamp. 7 d is generous.

## 3. Determinism
- Removed the 300 ms and 1.5 s sleeps in `fetch_shared_connection.rs`: `until(cond)` polling (10 s deadline) and `quiet(conn)` (byte count unchanged over a 250 ms window). 5/5 stable, 6.4 s.
- Restart test bound 10 s -> 20 s.

## Unresolved questions (round 3)
1. Stage 3 still closes the restarted peer's fresh in-use clip/presence conn (needed per measurement). Acceptable, or investigate an iroh API to clear remote path state?
2. Refresh `list-synced-at` also from presence-push syncs and pairing completion?
3. Expose list age through FFI (additive) for an app banner?

## Round 4 (stamp freshness)

Commit: see final message (one commit on top of 1bad0ac).

1. Stamp on every sync: one choke point. `sync_device_list` now = `fetch_and_ingest` + `trust.mark_synced()` on success. `TrustManager` holds a `OnceLock` callback (`set_on_synced`, set in `Inner::open` to `ListFreshness::stamp`). Covers presence-push resync, pairing initiator (`ApiListBackend::sync`), startup sync, remove_device. Scattered stamps in account.rs (Ready) and devices.rs removed; genesis stamp (Created, not a sync) kept. Atomic 0600 write unchanged.
2. No stamp (legacy install): age clock starts at mtime of `history.db` (holds the pinned list), never "now". No file / no mtime: fail closed.
3. Stamp (or mtime) more than 5 min (`MAX_SKEW_SECS`) ahead of now: invalid, falls back to item 2 / fail closed. Within 5 min: age 0.
4. 502/503/504 stay `CoreError::Api` (fail-closed); documented on `age_if_allowed`.

Tests: unit (list_freshness_tests.rs, split out to keep file <200): legacy mtime recent/old, no db, future stamp +fallback, skew boundary. Integration: `presence_resync_refreshes_the_stamp_so_a_long_running_app_may_start_offline` (stamp file set to 8 d old, A pairs C, B's presence pump re-syncs, poll stamp file with wait_for, then backend stopped, B ensure_account Ready). Verified it fails (timeout) with `mark_synced` removed.

Gates: fmt ok, clippy -D warnings ok, `cargo test --workspace` twice rc 0 / 0 failures. p0 not rerun (net code untouched).

Unresolved questions:
- history.db mtime moves with any history write, so a stampless legacy install that keeps writing history offline could extend its window; it gets a real stamp on first successful sync, so this only affects the first offline start after upgrade. Acceptable?
- Joiner device (paired via peer, no backend sync) has no stamp until its first backend sync; it falls back to db mtime. OK?
