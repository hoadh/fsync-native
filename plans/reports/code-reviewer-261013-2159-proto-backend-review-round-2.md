# Code review round 2 — fsync-proto + fsync-backend
Score 8.5/10. Tests 82/82, clippy clean. Round-1: H1,H3,M1,M3,M7,L2,L6,L7,L12 fixed; M2,M4,M8 partial; M5,M6,M9 mostly.

## New / remaining — Medium
- N1 OTP send failure leaves live code, no resend for 10 min (`backend/src/auth/otp.rs:46-65`) → delete row on send failure; allow resend after ~60 s.
- N2 TRUST_PROXY_HOPS>0 trusts XFF from any peer (`backend/src/client_ip.rs:25-44`) → TRUSTED_PROXIES CIDR list; non-UTF8 header lines shift index.
- N3 access token not bound to live session family (`backend/src/auth/extractor.rs:37-49`) → EXISTS check on family.
- N4 old-epoch history never pruned (`backend/src/devices/repo.rs:96-105`).
- N5 no header-read timeout / conn cap; /health behind global cap (`backend/src/main.rs:33-40`).

## Low
N6 limiter O(n) retain when full · N7 no-op OTP starts consume per-email limit · N8 reset limit consumed before validation · N9 removed node flashes online before membership check · N10 names.rs: reject `< > " | ? *`, bidi/invisible, COM¹²³, CONIN$/CONOUT$ · N11 migration 0002 logs out existing sessions (pre-launch OK) · N12 chain endpoint truncates at 100 rows silently · N13 unordered hub notify across concurrent PUTs · N14 pending node not re-checked · N15 heartbeat unrate-limited.

## Accepted (document)
M4 lost refresh response → 409 then family revoke (hash-only storage).

## Unresolved
1. Proxy + firewalling of backend port? 2. Email notice on trust reset? 3. Refresh grace semantics OK?
