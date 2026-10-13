# Code review — fsync-proto + fsync-backend (round 1)
Score 7.5/10. Tests: proto 30/30, backend 23/23. No Critical.

## High
- H1 XFF first-hop spoofable → per-IP OTP limit bypass + unbounded keys; TRUST_PROXY=0 behind proxy → global DoS. `backend/src/auth/routes.rs:28-38`
- H2 Server generates/sees pairing code → code-mode PAKE does not stop malicious backend MITM; SAS must be mandatory in code mode (core/UI, phase 06). `backend/src/pairing/routes.rs:53-73`
- H3 Trust reset needs only bearer token; no step-up/rate-limit/notify. `backend/src/devices/routes.rs:71-82`

## Medium
- M1 WS max_frame_size default 16 MiB (`presence/ws.rs:37`)
- M2 no pre-auth socket cap, request timeouts, concurrency limit
- M3 rate limiter O(n) retain under global mutex, unbounded, shared window bug (`rate_limit.rs:22`)
- M4 concurrent/retried refresh revokes family — needs grace window (`auth/refresh.rs:50`)
- M5 refresh family never expires; WS outlives token/logout
- M6 device-list PUT unbounded history/devices/name length; name control/bidi chars
- M7 removed device stays connected on DB error (fail-open) and gets PeerUpdates (`presence/ws.rs:171`)
- M8 rel_path/FileMeta.name validation: Windows reserved names, trailing dot/space, lengths, counts (`fsync-proto/src/clip.rs:82`)
- M9 OTP victim lockout, no verify IP limit, timing leak on missing row (`auth/otp.rs:31`)

## Low
L1 TrustReset prompt spam via replayed genesis · L2 `seq+1` overflow (`device_list.rs:81`) · L3 advisory added_seq · L4 JWT no iss/aud/kid · L5 presence sig nonce only · L6 idempotent PUT retry → 409 · L7 peer Addrs not sanitized; Hello flood · L8 email local-part chars · L9 sqlx error logging / dev-mode in prod · L10 secrets not zeroized; no PG TLS · L11 DELETE account no step-up · L12 `as u32` in encode_frame. Test files > 200 lines (presence 236, auth 206, devices 201).

## Unresolved
1. Proxy contract (Forwarded vs XFF), deploy proxy? 2. Trust reset requires fresh OTP? 3. MAX_FRAME_LEN 1.25 MiB for pair/1? 4. Multi-instance in MVP?
