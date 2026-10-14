# SMTP OTP Live, Singapore Relay Up, revoke_session Shipped: Review Caught the Real Bugs

**Date**: 2026-10-15 03:40
**Severity**: High
**Component**: backend (SmtpSender, OTP cap), deploy (relay-only mode), fsync-core, fsync-ffi, apps/macos, apps/windows
**Status**: Ongoing. Merged and deployed. Open items below.

## What Happened

The Gmail SMTP OTP sender and the global `OTP_DAILY_SEND_CAP` (450 per 24 h, migration 0003) are live on cms-node. The owner confirmed a real OTP reached the inbox on 2026-10-15. The Singapore relay runs in relay-only mode on a shared host whose Caddy is owner-managed. Core gained multiple relays (`relay_urls`), `logout_confirmed`, and an engine-free `revoke_session`. macOS (Keychain) and Windows (DPAPI) keep a pending revoke when sign-out cannot reach the server and retry it at launch. Windows phase 14 merged after four review rounds (6.5, 9.2, 9.0, 9.6).

## The Brutal Truth

Several of these would have shipped broken. `relay_urls` was first placed mid-record in `EngineConfig`, which silently breaks positional C# records. Our own docs claimed existing hosts compile unchanged, which was only true for Swift. The SMTP review found that a fresh production `.env` shipped with `EMAIL_SENDER=log` and `ALLOW_LOG_EMAIL=1`, and that `deploy.sh --relay-only` would run against a full-stack host. Windows sign-out could throw `UnauthorizedAccessException` and leave the user half signed out. All of it came from code we had already called done.

## Technical Details

- SMTP review: 8.5, 9.3, 9.6. The hard part was timeouts. A client timeout after Gmail accepted DATA is ambiguous. Now `EmailError{msg, maybe_delivered}`: ambiguous keeps the code row and all counters; a definite failure releases them. Overall send timeout is 12 s.
- Pending-revoke race (M1): a launch retry could delete or overwrite a newer pending token saved by a concurrent sign-out. Fixed by comparing the refresh token under one lock before settling.
- Relay e2e, 8 MiB, Vietnam client: both relays 0.55 MiB/s vs Ohio-only 0.105 MiB/s.
- Gates: cargo 300 passed / 0 failed; swift 208; dotnet 144/144; macOS integration PASS.
- Flakes: a `file_read_ahead` timing assertion failed once under load; a mailpit test failed once under load; a Postgres container start failed once. None reproduced on rerun. I did not chase them.

## What We Tried

- Refunding quota on every error: rejected. Timeouts after delivery understated sends, so an attacker who could induce timeouts could push a victim past the per-email limit.
- `relay_urls` mid-struct: rejected after H1. Now last with `#[uniffi(default = [])]`. C# hosts must regenerate `fsync_ffi.cs`, update the positional call, and ship the DLL with the binding. An old binding against a new DLL fails at record decode.
- Launch retry calling `clear()`: replaced with compare-and-settle under a shared lock.

## Root Cause Analysis

We declared done on green unit tests. FFI record field order is a wire format that function checksums do not cover. The docs were written from one language's point of view. Ambiguous timeouts were treated as definite failures. Two writers (sign-out and launch retry) shared one secret with no lock.

## Lessons Learned

- FFI record field order is a wire format. Add new fields last and regenerate every binding in the same change.
- An external timeout after the request left is not a definite failure. Do not refund quota on it.
- Two writers to one secret need one lock and a compare before delete.
- Review finds what unit tests miss. Budget four rounds for a platform track.

## Next Steps

- **LAN test (S7)**: blocked on owner cabling. Mac and Windows must share one router. Tailscale is the accepted fallback.
- **Manual GUI QA** on macOS and Windows (owner, no date).
- **First CI run** on these branches (owner: unassigned).
- **Alerts** on OTP cap hits and relay health (owner: ops).
- **Transactional email provider** before real users (owner: product). A Gmail App Password is not a production sender.
- **History full-content accessor** and **dial timeout** (owner: core).
- **Token exposure**: a refresh token lives up to 30 d if the app is never relaunched after sign-out. Fix or accept as a product decision (owner: core).

**Status:** Ongoing. Merged and deployed. Open items above.
**File:** /Users/alex/Code/fsync-native/docs/journals/261015-0340-smtp-sg-relay-revoke-session.md
