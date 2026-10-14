---
title: "User decisions 2026-10-14: Gmail SMTP, SG relay, revoke_session, LAN"
status: in-progress
created: 2026-10-14
updated: 2026-10-15
parent: ../261013-2029-fsync-native-mvp/plan.md
---

# Decisions (owner, 2026-10-14)
1. OTP email via Gmail SMTP (smtp.gmail.com:587 STARTTLS, App Password). Resolves MVP Q3. DONE.
2. Second relay region = Singapore (Lightsail). Resolves MVP Q2. DONE.
3. Add additive FFI `revoke_session` now (hard logout after quit). Resolves MVP Q5. DONE.
4. Mac↔Windows must see each other on LAN; Tailscale accepted when LAN is unavailable. Resolves MVP Q8. Deferred by owner (S7).

# Work items (status 2026-10-15)
| # | Item | Owner paths | Depends on | Status |
|---|---|---|---|---|
| S1 | Backend `SmtpSender` (lettre, rustls), env `EMAIL_*`/`SMTP_*`, prod refuses `log` sender, `OTP_DAILY_SEND_CAP` 450/24 h (migration 0003); tests vs real SMTP (mailpit) | `backend/**`, `deploy/**` | - | DONE. Review 9.6, merged. Deployed cms-node release 519ec821e90f. Real OTP delivered to inbox (owner confirmed 2026-10-15) |
| S2 | Relay-only deploy mode for SG host (compose profile + deploy.sh flag + smoke) | `deploy/**` | S1 | DONE. Merged. SG Lightsail relay-only live at relay-sg-fsync.10z.one, TLS ok |
| S3 | Core: multiple relays (`relay_urls`, last field, default `[]`; home relay by latency) | `core/**` | - | DONE. Review 8.6→9.5, merged bb412e8 |
| S4 | Core+FFI `revoke_session(api_url, refresh_token)` engine-free; macOS persists pending revoke on quit (Keychain), retries at launch, drops after 5 fails / 30 d | `core/**`, `apps/macos/**` | - | DONE. Review 8.6→9.5, merged bb412e8. `logout_confirmed` added |
| S5 | Windows wiring for S3/S4: C# bindings regen, `FSYNC_RELAY` comma list, DPAPI pending revoke | `apps/windows/**` | S3, S4 | DONE. Review 8.8→9.5, merged 7cc8cfd. `dotnet test` on merged main (waso) 144/144 (CoreBindings 9, Interop 47, App 88). Note: `fsync_ffi.dll` is gitignored, rebuild with bindings |
| S6 | Wire both relays into app configs, e2e | owner + lead | S2, S3 | DONE. macOS comma list (FSYNC_RELAY / Info.plist `FsyncRelayUrl` / build script `--relay`), ca466f7; Windows via S5. Both relays: relay-only clip 1.18 s, 8 MiB via relay 0.55 MiB/s, hash OK (Ohio-only was ~0.105 MiB/s). SG-only: clip 0.42 s, 8 MiB 0.63 MiB/s. Release builds must pass both relay URLs |
| S7 | LAN: Mac and Windows on separate ISP routers (VNPT 14.191.x vs Viettel 171.225.x, both 192.168.1.0/24). Owner must put both on one router/switch; set Windows "Ethernet 2" profile Private; re-run Track X over LAN | owner + lead | owner cabling | BLOCKED (owner). Physical cabling deferred. Tailscale accepted as fallback |

# Gates
Review >= 9.5, 0 critical; cargo/swift/dotnet tests 100%; FFI change additive only (docs/ffi-contract.md versioning).
Status 2026-10-15: met for S1-S6 per sync facts. S7 is not a gate, it is an open item.
