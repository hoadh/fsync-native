# ADR 002 — Auth: own passwordless email OTP + JWT
Status: accepted — confirmed in validation S1 (2026-10-13)

## Decision
Email OTP (6 digits, hashed, TTL, rate-limited) → EdDSA JWT access token (15 min) + rotating refresh token (30 d) with reuse detection that revokes the whole family. OIDC (Apple/Google/MS) deferred to P2.

## Rationale
- Trust root is device keys (signed DeviceList), not the account: an auth compromise cannot add devices or read data.
- No per-MAU vendor cost; small surface. An `AuthProvider` trait keeps a swap to managed OIDC under one week.
