# Phase 09 backend report

Status: complete. `cargo test -p fsync-backend` 23 tests pass (4 unit + 19 integration vs real postgres:17 via testcontainers), clippy -D warnings clean, fmt clean. `docker build` OK (~5 min cold), compose smoke OK (health 200, otp/start 204).

## Files
- backend/Cargo.toml (lib+bin; deps added in-crate, root Cargo.toml untouched)
- backend/migrations/0001_init.{up,down}.sql
- backend/src: lib, main, config, error, state, db, util, rate_limit; auth/{otp,tokens,refresh,routes,extractor}; devices/{repo,routes,verify}; pairing/{repo,routes}; presence/{hub,ws}; email/sender
- backend/tests: common/{mod,devices,ws}, auth, devices, pairing, presence
- deploy/backend/{Dockerfile, Dockerfile.dockerignore, compose.yml, .env.example}; docs/api.md
- All src files < 200 lines. Test files auth/devices/presence slightly exceed 200 (tests only).

## Deviations / decisions
- JWT hand-rolled (ed25519-dalek, fixed header, verify_strict) instead of jsonwebtoken: fewer deps, no alg confusion.
- Emails stored lowercased in plain `text` (no citext). Timestamps = bigint unix ms.
- OTP/pairing codes hashed with keyed blake3 (key derived from JWT seed) so DB leak does not allow offline guessing of 6 digits. Pairing hash bound to account id.
- No `AuthProvider` trait (YAGNI); `TokenService` + `AuthAccount` extractor are the seam. `EmailSender` and `PresenceHub` are traits.
- "5 lookups/code" not implemented separately: single use + 30/h/account (counting misses) bounds guessing. Pairing create also limited 30/h/account.
- Rate limiter in-memory (single instance); resets on restart.
- PUT If-Match: absent/`none` = expects no stored list. Trust reset ignores If-Match. `ChainResponse{chain:[b64]}` defined in backend (devices::routes), not fsync-proto.
- Pending (pairing) presence nodes: empty Peers, no updates; visible to listed devices. Removed devices are kicked after DeviceListChanged.
- Added: GET /health, device-list/chain, 64 KiB body limit, hourly purge of expired rows.
- Test containers: testcontainers-rs has no reaper; harness removes container via libc::atexit (one `unsafe` in tests, commented).
- `cargo fmt --all` was run workspace-wide (fmt check requirement); may have touched core/** formatting.

## Not done / concerns
- 10k-socket load test skipped (needs separate harness); hub uses std Mutex + bounded mpsc(64) per socket, drop-on-full.
- No real email provider (LogSender only). Postgres TLS not enabled in sqlx (add `tls-rustls` if DB is remote).
- Access token checked at WS connect only; sockets outlive token expiry.
- Offline hub entries for listed devices kept in memory until account deleted/process restart (small).
- Suggest moving `ChainResponse` into fsync-proto::api_dto (core owner) so clients share it; no proto change otherwise needed.
- Per-IP limit uses peer addr unless TRUST_PROXY=1; deploy must set it correctly behind a proxy.

## Review fixes (round 1: plans/reports/code-reviewer-261013-2116-proto-backend-review.md)
Tests: backend 48 pass (14 unit + integration vs postgres:17), proto all pass; clippy -D warnings, fmt, all files < 200 lines OK. Migration `0002_refresh_session` (refresh_tokens.auth_time, rotated_at).
- H1: `TRUST_PROXY` replaced by `TRUST_PROXY_HOPS` (default 0 = peer addr; N = Nth from right of XFF; bad/missing -> peer; IPv6 /64) in `client_ip.rs`. Documented in .env.example, compose, docs/api.md.
- H2: backend doc only (code-mode pairing relies on mandatory SAS; QR preferred). Core enforcement left to core.
- H3/L11: access JWT has `sid` + `auth_time`; trust reset + DELETE /account need auth within 5 min (401 `reauth_required`); reset 3/day; success -> DeviceListChanged to online sockets (removed ones closed).
- M1/M2: ws max_frame = max_message = 8 KiB; global unauth-WS cap (1000) + per-IP (10); HTTP routes get 15 s timeout (408) and in-flight cap (503). DEVIATION: custom axum middleware (`limits.rs`) instead of tower-http TimeoutLayer/ConcurrencyLimit (avoids new dep; ConcurrencyLimit queues, we fail fast).
- M3: rate limiter per-key expiry, 100k-key hard cap with eviction, pruning at most every 30 s.
- M4/M5: 10 s rotation grace -> 409 `refresh_in_progress` (no revoke); 90 d absolute family life; 20 families/account (oldest revoked); WS closes at access exp (4401), on logout/family revoke/account delete.
- M6: PUT limit 30/h; shape caps (<=32 devices, name <=64, no control/bidi) in fsync-proto `check_shape` (used by verify); history pruned to 100 rows per account+epoch.
- M7: membership checked before sending Peers with one DB retry, fail closed (1011); hub closes removed nodes directly (4401), no more peer updates to them.
- M8: proto (see proto report).
- M9: repeat `start` keeps live code (204 no-op); verify per-IP 30/h; dummy compare when no row.
- Low: L2 checked_add (proto); L6 Unchanged before If-Match; L7 direct addrs sanitized (DEVIATION: loopback/unspecified/multicast/port-0 are filtered out, not a close; >16 addrs or non-https relay (http in dev) closes), Hello 10/min per socket (close 4429); L9 sqlx errors logged by kind only, dev mode refused when `FSYNC_ENV=production`; L10 signing seed + hash key `Zeroizing`; L12 try_from.
- Tests split: auth_otp, auth_refresh, account_delete, devices_{chain,reset,limits}, presence_{auth,updates,limits}, pairing. New cover: concurrent refresh, double PUT race (join!), reset w/o fresh auth, reset 3/day + broadcast, oversize WS frame, XFF hops, shape caps, history prune, token-expiry close, logout close, unauth cap, hello flood/sanitize.
- Source split for line limit: presence/hub_{api,entry}.rs.

Still open: proxy contract (XFF assumed; choose proxy + hop count at deploy), multi-instance (hub, rate limiter in-memory; refresh/auth state in PG is fine), MAX_FRAME_LEN question for pair/1 (core), L1/L3/L4/L5/L8 untouched (low), PG TLS not enabled. Unauth-WS per-IP cap uses the same hop-resolved client key.

## Review fixes round 2
Source: plans/reports/code-reviewer-261013-2159-proto-backend-review-round-2.md. Tests: 101 pass (backend + proto); clippy -D warnings clean for backend + proto (with `-A clippy::doc-lazy-continuation`: other agent's in-progress `core/fsync-proto/src/file.rs:14` trips it); rustfmt clean on owned files (`cargo fmt --all` blocked by other agent's missing fsync-core modules); `cargo check -p fsync-core --tests` fails only on other agent's missing `clip/engine/file` modules (ChainResponse change is additive, `#[serde(default)]`).

- N1 `auth/otp.rs`: send failure deletes the inserted row (by code hash) -> 500; live code >= 60 s old is replaced (created_at derived as expires_at - TTL, no migration); younger = 204 no-op.
- N2 `client_ip.rs`, `config.rs`: `TRUSTED_PROXIES` CIDRs (ipnet); XFF honored only if peer in list; startup fails if hops>0 and list empty; any non-UTF8 XFF line -> peer. `client_key(&Config, ..)`. Compose + `.env.example` updated.
- N3 `auth/extractor.rs`: one indexed EXISTS on `refresh_tokens(family_id, account_id, live)` (implies account exists via cascade); used by HTTP + WS handshake.
- N4 `devices/repo.rs`: store deletes older-epoch history.
- N5 new `serve.rs`: hyper-util auto builder, HTTP/1 `header_read_timeout` (10 s, `Config.header_read_timeout`), upgrades kept, graceful drain 10 s. `/health` moved outside the in-flight guard. New deps: ipnet, hyper, hyper-util, tower (Cargo.lock updated). No connection cap (documented: firewall to proxy).
- N6 `rate_limit.rs`: at-capacity expired-key scan at most 1/s (tests split to `rate_limit/tests.rs`); added `peek`/`record`.
- N7 per-email limit: peek before insert, record only when a code is created. N8 reset budget: peek early, record after commit (idempotent re-PUT no longer counts).
- N9 `presence/ws.rs`: membership check before `hub.connect`. N14: pending sockets re-checked every `pending_recheck` (30 s) -> closed 4401 when offer expired and not listed (`handshake::pairing_state`). N15: `MsgLimiter`, heartbeats 6/min (close 4429), hello 10/min.
- N10 `core/fsync-proto/src/names.rs`: reject `< > " | ? *`, bidi/invisible (+ soft hyphen, U+2028/9, tag chars) in components; COM/LPT with superscripts; CONIN$/CONOUT$. Device-name invisible set extended too. Tests in `core/fsync-proto/tests/names_and_caps.rs`.
- N12 `api_dto::ChainResponse` +`earliest_seq: u64`, `truncated: bool` (serde default); repo fetches limit+1.
- M4, N11 documented in `docs/api.md` (also TRUSTED_PROXIES, header timeout, behavior changes).
- Tests added: otp send failure / 60 s resend / no-op starts (`auth_otp_limits.rs`), untrusted XFF (+ unit tests incl. non-UTF8), `session_revocation.rs` (HTTP + WS), epoch pruning, failed-reset budget, chain earliest/truncated, `header_read_timeout.rs` (slowloris, WS via serve, /health exempt), heartbeat flood, pending expiry. Existing tests adapted: `stale_session` / expiring-token test reuse the live family; limits test asserts epoch pruning.

Unresolved: N13 (unordered hub notify) left as-is; no global connection cap; no email notice on trust reset.
