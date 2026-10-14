# Phase 09 — Backend: auth, device registry, pairing codes, presence

## Context Links
- Design §2 Identity, §4 flows 1-2; ADR 001/002 ([phase-01](phase-01-repo-scaffold-and-decisions.md))
- Client: [phase-06](phase-06-core-identity-trust-transport.md) `api_client.rs`

## Overview
- Priority: P0 | Status: Partial | Effort: 3-4 wk (B)
- Rust axum service + Postgres: passwordless email OTP auth, signed device list storage (opaque to trust, but verified as defense-in-depth), pairing code rendezvous, WebSocket presence + address hints for discovery.

## Key Insights
- Backend is NOT trust root: it can deny service but not add devices or read data. Keep it small.
- Presence also distributes iroh addressing info (relay URL + direct addrs) → replaces n0 DNS discovery.
- Single instance in MVP; presence in-memory per process. Multi-instance later via Postgres LISTEN/NOTIFY (YAGNI now; keep `PresenceHub` trait).

## Requirements
- p95 API latency < 100 ms; presence update fan-out < 500 ms; 10k concurrent WS on 1 small VM.
- Rate limits: OTP start 5/h/email + per-IP; pairing lookup 5 attempts/code, 30/h/account.

## Architecture
### HTTP API (JSON, `Authorization: Bearer <access>`)
```
POST /v1/auth/otp/start   {email}                       → 204
POST /v1/auth/otp/verify  {email, code}                 → {access, refresh, account_id}
POST /v1/auth/refresh     {refresh}                     → {access, refresh}   (rotation, reuse detection → revoke family)
POST /v1/auth/logout      {refresh}                     → 204
GET  /v1/device-list                                    → {signed: b64, seq}  | 404 none
PUT  /v1/device-list      If-Match: <seq>  {signed: b64} → 200 | 409 seq conflict | 422 invalid chain
POST /v1/pairing          {node_id, relay_url}          → {code, expires_at}
GET  /v1/pairing/{code}                                 → {node_id, relay_url} (single use, same account only)
DELETE /v1/account                                      → 204 (GDPR)
WS   /v1/presence
  C→S Auth{access, node_id, sig(node_id key, "fsync-presence-v1"||server_nonce)}   // binds socket to device key
  C→S Hello{addrs: {relay_url, direct: [SocketAddr]}}; Heartbeat every 20 s (timeout 60 s)
  S→C Peers[{node_id, online, addrs, last_seen_ms}]; PeerUpdate{…}; DeviceListChanged{seq}
```
Presence only shows devices in account's current list; WS auth rejects node_id not in list (except during pairing).

### Postgres schema (sqlx migrations)
```
accounts(id uuid pk, email citext unique, created_at)
otp_codes(email, code_hash, expires_at, attempts)
refresh_tokens(id, account_id, family_id, token_hash, expires_at, revoked_at)
device_lists(account_id pk, seq bigint, signed bytea, updated_at)       -- latest only
device_list_history(account_id, seq, signed bytea, created_at)          -- for chain gap fetch + audit
pairing_codes(code_hash, account_id, node_id, relay_url, expires_at, used_at)
```
### Modules
```
backend/src/{main.rs, config.rs, error.rs, state.rs}
backend/src/auth/{otp.rs, tokens.rs, provider.rs /*AuthProvider trait*/, routes.rs, middleware.rs}
backend/src/devices/{routes.rs, repo.rs, verify.rs /*uses fsync-proto*/}
backend/src/pairing/{routes.rs, repo.rs}
backend/src/presence/{ws.rs, hub.rs, protocol.rs}
backend/src/email/sender.rs   backend/migrations/*.sql   backend/tests/*.rs
```
Data flow: client → axum → auth middleware → repo (sqlx) → Postgres; device-list PUT → `fsync_proto::verify(new, current)` → store + notify hub → WS DeviceListChanged to account devices.

## Related Code Files
- Create: files above, `deploy/backend/{Dockerfile, compose.yml, .env.example}`
- Modify: none (fsync-proto owned by A; B requests changes via PR to A)

## Implementation Steps
1. Skeleton, config, error mapping, health endpoint, migrations.
2. OTP auth + JWT (EdDSA) + refresh rotation; email sender (provider API, dev = log sink).
3. Device list GET/PUT with If-Match + chain verify + history.
4. Pairing codes (hash stored, TTL, single-use, rate limit).
5. Presence WS hub with device-key challenge; heartbeat/timeouts.
6. Integration tests with testcontainers Postgres; OpenAPI doc in `docs/api.md`.

## Todo List
- [x] Skeleton + migrations
- [x] OTP auth + refresh rotation
- [x] Device list endpoints
- [x] Pairing codes
- [x] Presence WS
- [x] Rate limiting
- [x] Integration tests + docs/api.md

## Implementation Notes (sync 2026-10-14)
- Done: Rust axum + Postgres 17 (sqlx). Email OTP, JWT hand-rolled on ed25519 (no jsonwebtoken), refresh rotation with reuse detection. Device list GET/PUT with If-Match and chain verify. Pairing codes (keyed blake3 hash, single use, TTL). Presence WS (challenge auth, Hello, heartbeat, WS ping, liveness). Rate limits. `docker build` and compose smoke OK. `docs/api.md`.
- Deviation: no `AuthProvider` trait (YAGNI). `TokenService` and `AuthAccount` extractor are the seam.
- Deviation: pairing lookup limited to 30/h/account. No separate "5 attempts per code".
- Deviation: history pruned to 100 rows per account and epoch. Rate limiter in memory (single instance).
- Hardening from reviews: TRUSTED_PROXIES CIDR list, trust reset needs auth under 5 min, WS closes at access-token expiry (4401).
- Open: Postgres TLS not enabled (add sqlx `tls-rustls` before a remote DB). No global TCP connection cap (proxy responsibility).
- Open: email is `LogSender` only. No real provider chosen (plan Q3).
- Flake: `devices_reset` hit `PortNotExposed` once in the final run. Root cause not found. Fix in `backend/tests/common/pg.rs` retry, not yet selective (review L3).
- Not done: 10k idle WS load test (<1 GiB RAM target).
- Trust reset endpoints (`PUT /v1/device-list` reset path, 401 `reauth_required` with 5-min fresh auth, 3/day cap) are covered by the M5 real-backend tests (2026-10-14). Email notice on reset not built.

## Success Criteria
- Integration tests pass: auth flow, 409 on seq conflict, 422 on forged/rollback list, code single-use + expiry, WS rejects non-listed node; load test 10k WS idle sockets < 1 GiB RAM.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Email deliverability for OTP | M x H | reputable provider, SPF/DKIM/DMARC in phase 10 |
| Own-auth security bugs | M x H | small surface, review checklist, rate limits; swap to OIDC via trait if needed |
| Presence lost on deploy restart | H x L | clients auto-reconnect w/ backoff; iroh connections stay up |

## Rollback
- sqlx migrations with paired down scripts; blue/green container deploy (phase 10); API versioned `/v1`.

## Security Considerations
- OTP & codes stored hashed; constant-time compare; JWT short-lived; refresh reuse detection; TLS only; CORS none (native clients); no clipboard/file data endpoints exist; logs exclude emails in plain (hash) beyond auth audit.

## Next Steps
- Phase 10 deploys; apps (11/13) integrate login + pairing.
