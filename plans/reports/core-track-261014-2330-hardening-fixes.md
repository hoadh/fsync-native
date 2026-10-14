# Core track: review fixes (round 2)

Branch `core/hardening`. Gates: fmt clean, clippy -D warnings clean, `cargo test --workspace` 261 passed / 0 failed.

## Items
1. Logout blocking (HIGH): inline phase = one attempt under a 3 s timeout. Test `logout_against_a_black_holed_backend_returns_fast_and_revokes_later` (TCP forwarder that accepts and never answers): logout < 6 s, revoked after backend returns.
2. Logout vs in-flight refresh (MEDIUM): `ApiClient` epoch bumped by `take_refresh()`; token store + `on_tokens` callback happen under the token lock only if the epoch is unchanged; 401 only clears tokens if epoch unchanged. Test `a_refresh_in_flight_cannot_store_tokens_after_logout` (mutation-checked: fails with the check disabled).
3. Revoke retries (MEDIUM): background retry now unbounded while the service lives (5 s doubling to 5 min), ends on success/definitive answer/service drop. Earlier "24 tries" wording was wrong and is gone. In-memory only: if the app quits first the token lives until its 30 d expiry.
4. Offset 0 (MEDIUM): sequential = `offset == last_end` only. No dedicated test (not observable without internals); existing read-ahead tests still pass.
5. Prefetch budget (MEDIUM): `PREFETCH_PER_PEER = 8` permits per peer (`reader_cache.rs`), prefetch uses `try_acquire_owned` and never waits; read-ahead depth AHEAD = 4 windows of 4 MiB. Unit test `prefetch_budget_is_one_per_peer_and_bounded`.
6. Cleanup (LOW): `take_refresh` in `api/client.rs`, `tokens` field private again, poisoned lock logged with `warn!` and recovered, `ApiClient::logout` removed (`tests/e2e_api.rs` uses `take_refresh()` + `revoke()`). client.rs split (`api/check.rs`, `api/client/authed.rs`) to stay under 200 lines.
7. Data-dir lock: `<data_dir>/engine.lock`, exclusive, taken in `start()`, released on `stop()`/drop. Used std `File::try_lock` (stable, cross-platform) instead of fs4: no new dependency. Maps WouldBlock to existing `FsyncError::Invalid`; no new variant, no FFI change. Tests `data_dir_lock.rs` (second start fails; succeeds after stop; lock file reusable).

## Proposed doc wording (coordinator applies at merge)
`docs/ffi-contract.md` line 29 (`logout() throws`): "Signs out locally first (tokens, pinned trust, session) and returns Ok once that is done; it does not surface network errors. Server-side revocation of the refresh token is attempted once inline (bounded to 3 s) and, if the backend is unreachable, retried in the background (5 s doubling to 5 min) for as long as the service object lives. The refresh token is never persisted by core: if the app quits before revocation succeeds, it stays valid until its 30-day expiry."
`docs/ffi-contract.md` (`start()`): "Fails with `Invalid(\"another fsync engine is already running on this data directory\")` if another engine holds the exclusive lock on `<data_dir>/engine.lock`. The lock is released on `stop()` or when the service is dropped."
`docs/api.md` (`POST /v1/auth/logout`): "Revokes the refresh token. The core client treats 401 as already revoked and retries on network failure (see ffi-contract logout)."

## Unresolved
- Hard revoke guarantee after app quit needs an additive FFI `revoke_session(refresh)` (app persists pending token). Not done: FFI additive change needs coordinator approval.
- Windows track should re-run the 200 MiB shell paste with read-ahead.

## Round 3 (re-review lows)
Gates: fmt, clippy -D warnings clean; `cargo test --workspace` 264 passed / 0 failed.
- L1: `SeqRun` (`file/seq_run.rs`) keeps `last_end: Option<u64>`; first read never sequential. In-module unit test `first_read_is_never_sequential_even_at_offset_zero`.
- L2: `Inner.start_gate` (tokio mutex) serializes `start` and `stop`. Test `concurrent_starts_on_one_service_all_succeed` (6 threads).
- L3: `logout` takes the refresh token first, then signs out locally; local errors are returned after revoke is queued/attempted.
- L4: if the engine is still referenced after the 50-try wait, a background task keeps the data-dir lock, waits until the engine is unique, then shuts it down cleanly and releases the lock. No overlap with a new engine.
- L5: `on_tokens` now runs outside the token lock (reading `tokens()` inside is safe). Ordering vs logout kept by a separate `notify` mutex that `take_refresh` also takes first: logout waits for a running callback, so it must not be called from `on_session_changed` (or by something that blocks on it). Proposed `docs/ffi-contract.md` wording for `on_session_changed`: "Called on the delegate queue with each rotated token pair; do not call `logout()` synchronously from this callback and wait for it. After `logout()` returns, no further `on_session_changed` is delivered for a refresh that began before it." (The delegate is dispatched through a queue, so the engine-side callback only enqueues; no deadlock on re-entry.)
- Open question (answered): backend `refresh::revoke` resolves the family from the presented token hash (`family_id IN (SELECT family_id ... WHERE token_hash = $1)`, no revoked/rotated filter) and sets `revoked_at` on every live token of that family. A token rotated by an abandoned in-flight refresh therefore dies with the family. No backend src change. Added test `logout_kills_the_whole_family_including_rotated_tokens` (backend/tests/auth_refresh.rs): refresh once, logout with the old rotated token, newer token gets 401.
