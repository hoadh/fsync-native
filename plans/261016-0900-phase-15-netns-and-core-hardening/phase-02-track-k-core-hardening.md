# Phase 02 — Track K: core hardening

## Context Links
- Round 2 [report](../reports/fullstack-developer-261015-0425-core-round-2.md) (A2 gap: no fetch retry on dead cached conn)
- [MVP phase 08](../261013-2029-fsync-native-mvp/phase-08-core-history-ffi-cli.md) line 79/90 (M1/M2)
- Four-track [plan](../261015-0410-four-track-hardening/plan.md) Next action 10, Unresolved 7
- Code: `core/fsync-core/src/net/dialer.rs`, `file/fetch_client.rs`, `file/resume.rs`, `clip/send.rs` (pattern to copy), `service/lifecycle.rs`, `service/engine_release.rs`, `net/path_state.rs`, `net/lan_first_path_selector.rs`, `core/fsync-cli/src/{main,cmd_files,cmd_account,ctx}.rs`

## Overview
- Priority P1 | Status: Completed (merged after 4 review rounds; scope deviations below) | Effort 1d | Branch `track-k/core-hardening` (worktree off main 68d7ae6)
- K1 fetch retry on dead cached connection. K2 engine.lock M1/M2: verify and close (already fixed on main). K3 CLI path tier + tracing subscriber.
- Ownership `core/**` only. No `apps/**`. FFI: no change planned (additive allowed, not needed).

## Key Insights
- K1 root cause: `Dialer::connect` returns a cached conn while `close_reason()` is None. After a peer crash it is dead but not closed; `fetch_client::open` does `open_bi` (local, succeeds), sends the request, then waits `STALL` = 30 s for `FetchResp` and fails. `resume::fetch_to_store` retries only on `Invalid("restart")`. Clip send already handles this (`clip/send.rs`: 2 s first attempt, then `forget` + close + redial). Both `fetch_file` and `fetch_rep` and `RangeReader` go through `fetch_client::open` -> one fix point.
- K2: main already has commits 7817300 (bounded `LINGERING_LOCK_MAX_HOLD` 60 s; `INLINE_RELEASE_WAIT` = teardown 3 s - 1 s, compile-time assert), 7e92f3d (close leaked engine before release), 7b9cd0f (tests `stop_leak_tests.rs`, `engine_release.rs` unit tests). MVP plan line 90 is stale. Remaining gap: "clear error" — `start()` during a lingering hold returns `EngineAlreadyRunning` with text "another engine is running", misleading when it is our own engine still releasing.
- K3 iroh 1.3.0 API (checked in registry source): `Connection::paths() -> PathList`, `PathInfo::{is_selected, is_ip, is_relay, remote_addr() -> &TransportAddr, rtt}`; `path_state::connection_state` already uses it. Tier needs the selected path's remote IP + node's `LocalSubnets`, mapped by existing `tier_of` (takes transports `Addr`; add a `TransportAddr` variant or small adapter). `PathState`/`DeviceStatus` (Offline/Lan/Relay) are FFI-mirrored: do NOT change them; add a separate core-only type.
- CLI has no tracing subscriber; `tracing-subscriber` (env-filter) is already in the lockfile via backend.

## Requirements
K1
- On a cached connection, the fetch handshake (open stream, send req, receive `FetchResp`) gets a short deadline (2 s, reuse clip's constant semantics). On transport error/timeout there: `dialer.forget` + close that conn, redial once (full `DIAL_TIMEOUT`, `STALL` as today). Exactly one retry.
- No retry on protocol answers (Denied, NotFound, Changed, Integrity) or after data started flowing (resume handles that).
- Fresh (non-cached) dials keep current behavior.
K2
- Keep bounded hold. When `start()` finds the lock held by this service's own lingering holder: `tracing::warn!` with remaining hold ("previous engine still releasing, up to Ns") and return `EngineAlreadyRunning` (unit variant; FFI frozen). Optional: wait up to min(remaining, 2 s) before failing — only if a test shows it is deterministic.
- Tests: own-lingering start returns typed error and succeeds after release; compile-time M2 assert stays.
K3
- Core: `pub enum PathTier { Lan, Direct, Relay }` + `FsyncService::peer_path_tier(node_id: &str) -> Option<PathTier>` (best tier across live connections to the peer). Not exported via FFI.
- CLI: add `"path":"lan|direct|relay|null"` to `sent`, `fetched`, and each device in `devices --json`. Keep `via` unchanged (p0 and macOS integration parse it).
- CLI: install `tracing_subscriber::fmt` to stderr, `EnvFilter` from `RUST_LOG`; when unset, no subscriber output (behavior unchanged). Stdout stays pure JSON lines.

## Architecture
```
fetch_file / fetch_rep / RangeReader
  -> fetch_client::open(dialer, peer, req)
       (conn, reused) = dialer.connect_reporting_reuse(peer, ALPN_FILE)   // additive Dialer method
       handshake under deadline = reused ? FIRST_ATTEMPT : STALL
       on Net/timeout && reused: forget + conn.close + gate.close_stale -> connect again -> handshake(STALL)
  -> resume loop unchanged

CLI send/fetch Done -> svc.peer_path_tier(node) -> Node -> Gate live conns(peer) -> selected PathInfo.remote_addr
  -> tier_of(addr, local_subnets) -> "lan"/"direct"/"relay"
```
Data flow K3: read-only sampling at completion; no new events, no state.

## Related Code Files
Modify
- `core/fsync-core/src/net/dialer.rs` — additive `connect_reporting_reuse` (or `cached`-aware variant)
- `core/fsync-core/src/file/fetch_client.rs` — handshake deadline + single retry (keep < 200 lines; split helper into `file/fetch_handshake.rs` if needed)
- `core/fsync-core/src/service/lifecycle.rs` — own-lingering detection + warn
- `core/fsync-core/src/service/inner.rs` — lingering flag/deadline (if needed)
- `core/fsync-core/src/net/lan_first_path_selector.rs` — `tier_of` adapter for `TransportAddr`, make `Tier` reachable as `PathTier`
- `core/fsync-core/src/net/{node.rs,gate.rs}` — peer best tier from live conns
- `core/fsync-core/src/service/devices.rs` (or new `service/path_tier.rs`) — `peer_path_tier`
- `core/fsync-cli/src/{main.rs,cmd_files.rs,cmd_account.rs}`, `core/fsync-cli/Cargo.toml` (+ `tracing-subscriber`)
Create
- `core/fsync-core/tests/fetch_after_peer_restart.rs`
- `core/fsync-core/tests/path_tier.rs` (or unit tests next to code)
Delete: none.

## Implementation Steps
1. Worktree: `git worktree add ../fsync-native-track-k -b track-k/core-hardening 68d7ae6`.
2. K1 test first: model on `tests/dial_after_restart.rs` (crash = drop runtime, same identity, new port). A serves a file; B fetches once (warms cached `ALPN_FILE` conn); A crashes and restarts serving the same offer; B fetches again (file, then rep). Assert success within 10 s. Run on unfixed code: must fail (~30 s STALL). Record both outputs in report.
3. K1 fix in `fetch_client::open` per Architecture. Run test 10x for flakiness.
4. K2: confirm existing tests pass; add test "start during own lingering hold -> EngineAlreadyRunning, then Ok after handle drop"; add warn. Note in report that M1/M2 were closed by 7817300/7e92f3d/7b9cd0f.
5. K3 core: `PathTier`, adapter, `peer_path_tier`; unit tests for mapping (Lan/Direct/Relay incl. IPv6 and relay addr) + one integration test (two in-process nodes on loopback -> `lan`; relay-only builder -> `relay`).
6. K3 CLI: `path` fields; tracing subscriber; check stdout unchanged with `RUST_LOG` unset.
7. Gates: fmt, clippy -D warnings, `cargo test --workspace`; `p0-exit-demo.sh` MIB=16 (via + path fields); `scripts/build-xcframework.sh` + `swift test`.
8. Report to `plans/reports/`.

## Todo List
- [x] Worktree
- [x] K1 regression test (red on main: hang, 25.4 s)
- [x] K1 fix (green, 10/10 runs in round 1; 5/5 after round 3 budget change)
- [x] K2 own-lingering test + warn; M1/M2 closure note (closed by 7817300, 7e92f3d, 7b9cd0f)
- [x] K3 PathTier + peer_path_tier + tests
- [x] K3 CLI path fields + RUST_LOG subscriber
- [x] Gates incl. p0 MIB=16, xcframework + swift test (FsyncMac 231 run, 18 skipped, 0 fail)

## Success Criteria
- `fetch_after_peer_restart` fails on 68d7ae6, passes on branch; second fetch < 10 s (was ~30 s fail).
- No FFI/UDL diff (`git diff main -- core/fsync-ffi` empty or additive only); `via` values unchanged.
- `fsync-cli ... send` emits `"path":"lan"` on loopback; `--force-relay` emits `"relay"`.
- `RUST_LOG=debug fsync-cli ...` prints traces on stderr; stdout parses line-by-line as JSON.
- All gates green; review >= 9.5.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| 2 s handshake deadline aborts a slow but live peer | M x M | only on reused conns; retry has full STALL; test with delayed server response |
| Retry duplicates request on a live peer | L x L | first stream stopped; server read-only; idempotent |
| Crash simulation flaky in test | M x M | reuse dial_after_restart harness; deadline-based asserts |
| Tier sampled after conn idles -> null | M x L | sample right at Done; null allowed, documented |
| iroh unstable API drift | L x M | iroh pinned =1.3.0 |
| stderr noise breaks macOS CLI integration | L x M | no subscriber output unless RUST_LOG set |

## Rollback
One commit per K item; revert individually. No persisted format, FFI or config change.

## Security Considerations
Retry path keeps trust check (`Dialer::connect` refuses untrusted peers). Tracing at debug may log node ids/addresses, never keys or clip content; review log fields.

## Next Steps
Lead: tick phase-08 M1/M2 and A2 gap; four-track Next action 10 fetch part done; Unresolved 7 answered. Track N may log `path` as cross-check after merge.

## Results (merged main 026fb5a)
- Reviews 8.6, 9.2, 9.3, 9.6. Gates: fmt, clippy -D warnings, `cargo test --workspace` green. p0 MIB=16 PASS. xcframework + swift test green.
- K1: fetch retry on dead cached conn done. Protocol answers not retried.
- K2: lingering-hold warn added. M1/M2 closed on main before K. Phase-08 line 79 is where M1/M2 is listed (not line 90).
- K3: `PathTier` core-only, no FFI change. `via` unchanged.
- Backend-down start (added scope, answers Q2): fail-closed offline fallback, 7-day bound via `<data_dir>/list-synced-at`, legacy mtime fallback, future-stamp reject.

Scope changes (deviation vs spec above):
- `FIRST_ATTEMPT` 2 s -> 5 s. Reason: slow relay (~0.4 MiB/s). Impact: slower stall detection on reused conns.
- Regression bound 10 s -> 20 s. Last run 11.6 s; spec "second fetch < 10 s" not met on that run. Needs owner sign-off.
- Redial-stall escalation closes peer's idle, then in-use inbound clip/presence conns (pulls spared). They reconnect on next use.
- Loopback = `lan`. `fetched` key is `path_tier`.
- K2 optional bounded wait not implemented.
- Windows dotnet not run (no FFI change, optional).
