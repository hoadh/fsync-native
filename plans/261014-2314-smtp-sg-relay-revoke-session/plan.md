---
title: "User decisions 2026-10-14: Gmail SMTP, SG relay, revoke_session, LAN"
status: in-progress
created: 2026-10-14
parent: ../261013-2029-fsync-native-mvp/plan.md
---

# Decisions (owner, 2026-10-14)
1. OTP email via Gmail SMTP (smtp.gmail.com:587 STARTTLS, App Password). Resolves MVP Q3.
2. Second relay region = Singapore (Lightsail). Resolves MVP Q2.
3. Add additive FFI `revoke_session` now (hard logout after quit). Resolves MVP Q5.
4. Mac↔Windows must see each other on LAN; Tailscale accepted when LAN is unavailable. Resolves MVP Q8.

# Work items
| # | Item | Owner paths | Depends on | Status |
|---|---|---|---|---|
| S1 | Backend `SmtpSender` (lettre, rustls), env `EMAIL_*`/`SMTP_*`, prod refuses `log` sender; tests against a real local SMTP (mailpit container) | `backend/**`, `deploy/**` | - | in progress |
| S2 | Relay-only deploy mode for a dedicated SG host (compose profile + deploy.sh flag + smoke) | `deploy/**` | S1 (same owner) | in progress |
| S3 | Core: multiple relays (`relay_urls`, iroh RelayMap; home relay by latency), additive FFI field with default | `core/**` | follow-ups merged (ca6d57d) | in progress |
| S4 | Core+FFI `revoke_session(api_url, refresh_token)` free fn; macOS host persists pending revoke on quit, retries at next launch | `core/**`, `apps/macos/**` | follow-ups merged (ca6d57d) | in progress |
| S5 | Windows host wiring for S3/S4 (C# bindings regen, pending revoke) | `apps/windows/**` | Track B merge, S3/S4 | pending |
| S6 | Provision SG Lightsail + DNS `relay-sg-fsync.10z.one`, deploy relay-only, wire both relays into app configs | owner + lead | S2, S3 | host ready 2026-10-15: shared (host Caddy serves another site), no Docker yet; DNS ok. Mac RTT 81 ms vs 281 ms Ohio, SSH download 0.36 vs 0.10 MiB/s. Needs owner Caddy snippet + Lightsail firewall rule |
| S7 | LAN: diagnosed: Mac and Windows sit on two separate ISP routers (VNPT 14.191.x vs Viettel 171.225.x), both 192.168.1.0/24. Owner must put both on one router/switch; then set Windows "Ethernet 2" profile Private (currently Public) and re-run Track X over LAN | owner + lead | owner cabling | blocked: physical |

# Gates
Review >= 9.5, 0 critical; cargo/swift/dotnet tests 100%; FFI change follows docs/ffi-contract.md versioning (additive only).
