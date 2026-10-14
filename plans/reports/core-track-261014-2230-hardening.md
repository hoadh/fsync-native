# Core track C: hardening (branch core/hardening, base 85f8968)

Commits: da5ed8a read-ahead, 2b2f72d pg port flake, 456f60f logout revocation (all pushed). No FFI signature change, no additive FFI change, no bindings regen needed.

## 1. Read-ahead in `read_file_range`
Before: one prefetched window (4 MiB) -> throughput = 4 MiB / (RTT + transfer) for a serial 256 KiB reader.
After: from the 2nd sequential read, 4 windows (16 MiB) in flight (`JoinSet`, aborted on reader eviction); windows behind the reader dropped; random reads unchanged.
Measured: `tests/file_read_ahead.rs`, 200 MiB, 256 KiB reads one at a time, in-process peers on loopback, release build. `EngineConfig.serve_delay` (test-hooks only) delays each file/1 request = 1 RTT.

| RTT | before | after |
|---|---|---|
| 0 ms | 105.8 MiB/s | 109.6 |
| 50 ms | 45.8 | 104.5 |
| 100 ms | 27.9 | 66.6 |

Loopback is hash/copy bound at ~105-110 MiB/s. Regression test (debug-safe: compares against its own 0 ms CPU baseline) fails on the old code (4.22 s vs bound ~2.8 s), passes after.
Memory: up to 8 windows x 4 MiB = 32 MiB per active reader (same cap as before).
Not measured: real WAN/relay RTT, Windows shell end to end (needs the Windows track).

## 2. Logout revokes the refresh token
Backend already revoked on `POST /v1/auth/logout`; the bug was core: offline failure left tokens in `ApiClient` and no retry, and the app clears its own keystore regardless. No backend or migration change.
New behavior (`service/login.rs`, `service/revoke.rs`): local sign-out first (account, presence, tokens, pinned trust), then revoke: 2 inline retries (0.5 s, 1.5 s), then background retry (5 s doubling to 5 min, 24 tries) while the service lives. 401 = already revoked. Non-transient 4xx = give up, logged. `logout()` returns Ok after local sign-out (previously returned the network error; apps only logged it).
Limit (documented in code and phase 08): the token is never persisted by core (tokens live in the app keystore). If the app quits before the backend is reachable, the token stays valid until its 30 d expiry (access token 15 min). A hard guarantee would need the app to keep the refresh token and call a revoke on next start: possible later via a small additive FFI (`revoke_session(refresh)`), not done (YAGNI, no FFI change allowed now).
Test `tests/logout_revocation.rs`: real backend behind a TCP forwarder; forwarder cut, logout returns Ok, session still live server-side, forwarder restored, session becomes 401 within seconds. 3/3 green.
Docs not edited (not owned): `docs/ffi-contract.md` line 29 ("logout() throws") and `docs/api.md` could note this; lead to merge.

## 3. `PortNotExposed` flake: root cause
Reproduced (1 in ~20-30 postgres container starts, 3 times in ~100 starts, while sibling worktrees load Docker Desktop). The container is running and postgres is ready, but Docker never published the port: `docker inspect` shows `"5432/tcp": []` permanently, and the start itself takes ~6 s. It is not a read-too-early race, so the old 5 s retry in backend `pg.rs` never could help (probe waited 12 s). core's `backend_harness` had no retry at all (it flaked in my logout test).
Fix: `backend/tests/common/pg.rs::start_with_port`: retry only on `PortNotExposed` for 2 s, else remove the container and start a new one (4 attempts); other errors fail fast. core harness now `#[path]`-includes the same helper (removes duplicate code). Verification: 60 consecutive starts, 2 hits, both recovered on attempt 2.

## 4. e2e x3 on final tree (`tests/e2e/p0-exit-demo.sh`, unedited)
3/3 PASS (run 1 included a cold build, 182 s; runs 2-3: 40 s).

| run | direct clip | direct 256 MiB recv (via) | relay clip | relay 256 MiB recv (via) |
|---|---|---|---|---|
| 1 | 0.03 s | 34.5 MiB/s (Lan) | 0.06 s | 50.3 MiB/s (Relay) |
| 2 | 0.02 s | 38.4 (Lan) | 0.06 s | 52.0 (Relay) |
| 3 | 0.03 s | 38.8 (Lan) | 0.02 s | 42.8 (Relay) |

Observation for Track D: the final "devices as seen by A" step prints `online:false` for devB (informational, not asserted; CLI exits after pairing). No fix needed.

## Gates (final tree)
fmt clean; `cargo clippy --workspace --all-features --tests --benches -- -D warnings` clean; `cargo test --workspace` 255 passed / 0 failed. New files < 200 lines.

## Unresolved questions
1. Hard revoke guarantee when the app quits offline (see 2): want an additive FFI `revoke_session(refresh)` for apps to retry on next launch? Needs app tracks.
2. AHEAD=4 (16 MiB) fits ~50 MiB/s at 300 ms RTT; raise if Windows measurements over a real relay show a ceiling.
3. Windows track should re-run its 200 MiB shell paste against this core (ADR 010 numbers were serial-bound).
