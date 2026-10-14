# PM sync: S1-S6 done, S7 blocked (2026-10-15)

Main 461dd57, pushed to origin. Docs/plans only. No code, no docs/, no commits.

## Work item status (S1-S7)
| # | Status | Evidence |
|---|---|---|
| S1 Gmail SMTP sender + 450/24 h cap | DONE | Review 9.6, merged. cms-node release 519ec821e90f. Real OTP in inbox (owner) |
| S2 Relay-only deploy mode | DONE | Merged. SG relay-sg-fsync.10z.one live, TLS ok |
| S3 Core relay_urls | DONE | Review 8.6→9.5, merged bb412e8 |
| S4 revoke_session + macOS pending revoke | DONE | Review 8.6→9.5, merged bb412e8 |
| S5 Windows wiring (bindings, relay list, DPAPI revoke) | DONE | Review 8.8→9.5, merged 7cc8cfd. dotnet 144/144 on waso |
| S6 Two-relay config + e2e | DONE | ca466f7. Both relays: clip 1.18 s, 8 MiB via relay 0.55 MiB/s. SG-only: clip 0.42 s, 8 MiB 0.63 MiB/s |
| S7 LAN | BLOCKED (owner) | Physical cabling deferred. Tailscale accepted fallback |

## Files updated
- `plans/261014-2314-smtp-sg-relay-revoke-session/plan.md`: S1-S7 statuses, decisions marked done, deferred LAN.
- `plans/261013-2029-fsync-native-mvp/plan.md`: Progress rewritten (stale 10-14 bullets dropped), phase table, Q2/Q3/Q5/Q8 marked. Trimmed to 83 lines (target <80).
- Phase 09: Gmail SMTP done, smtp flake noted.
- Phase 10: SG relay done, Owner Action 5 updated, success status updated.
- Phase 11: pending revoke (S4) and relay list (S6) done, release relay rule added.
- Phase 12: status pointer to phase 11 for relay list and pending revoke.
- Phase 13: S5 done, dotnet 144/144, `fsync_ffi.dll` rebuild note.
- Phase 14: merged 37e0430, status and sync note.

Not touched: phase 15, 16 (pending, no new facts). `docs/` untouched.

## Scope changes (logged)
- Gmail SMTP chosen as OTP sender. Reason: owner decision 10-14. Impact: Workspace account is a single-provider dependency. Transactional provider still advised.
- SG chosen as second relay (US/EU not added). Impact: relay list is a comma list in all app configs.
- `revoke_session` pulled into MVP (was deferred). Impact: additive FFI, closes quit-before-revoke risk.
- Release builds must pass both relay URLs. Impact: build-script rule, see risks.

## Risks (updated)
- CLOSED: no OTP email provider (Gmail SMTP live). CLOSED: 2nd relay region undecided. CLOSED: quit-before-revoke leaves refresh token valid 30 d (S3/S4 both platforms).
- NEW: release build ships with one relay URL (M x M). Mitigation: build script fails without both, owner C/D.
- NEW: Gmail App Password / Workspace account outage stops OTP (M x H). Mitigation: transactional provider before real traffic, daily cap 450 monitored.
- OPEN: Mac↔Win LAN unreachable (S7, owner). Tailscale fallback only.
- OPEN: CI never run on GitHub (Win job unverified). Impact: no automated gate on origin.
- OPEN: no alerts/monitoring; no rollback rehearsal.
- OPEN: flakes: smtp_mailpit (1 fail/8), PG container start (core), Win `WithoutAnEngineSignOutRevokesDirectly` (review L2).

## Blockers (stalled >1 session)
- S7 LAN: owner. Unblock: put both machines on one router or switch, set Windows "Ethernet 2" to Private, re-run Track X over LAN. Done when: Mac↔Win clip+file pass over LAN.

## Next actions (finish the plan; every open item closes or is explicitly deferred)
1. Owner: S7 cabling. DoD: LAN Track X PASS, or owner signs Tailscale-only in Q8.
2. D: Win GUI pairing with Mac + ARM64 hardware run. DoD: GUI pair + status LAN/relay/offline within 5 s; ARM64 exe launches.
3. C: manual QA matrix (04/12) into ADR 009, tick phase 12 checklist, verify S4 retry on relaunch. DoD: recorded results, no open manual items.
4. A: M1/M2, dial-after-restart fix, history full-content accessor, typed engine.lock error. DoD: tests green, review >= 9.5.
5. B: first green GitHub CI run; alerts; rollback rehearsal; confirm `FSYNC_ENV=production` and SPF/DKIM/DMARC. DoD: each checked in phase 10.
6. C/D: enforce both relay URLs in release builds. DoD: build fails without both.
7. B: phase 15 harness, phase 16 release (signed launch-at-login). DoD: phase tables reach Done.
8. Lead: push after review of each track. Docs impact: minor (phase files + plans only, docs/ pending lead).

## Status
**Status:** DONE_WITH_CONCERNS
**Summary:** S1-S6 done and verified in facts. S7 blocked on owner cabling. Plan files updated to 10-15 state, scope changes and risks logged.
**Concerns:**
- Phase 14 file records review round 3 at 9.0; sync facts say 9.6. Needs reconcile.
- MVP plan.md is 83 lines (target <80).
- M1/M2 core status and phase 12 review-medium item not re-verified since 10-14.
- `FSYNC_ENV=production` switch and SPF/DKIM/DMARC status not in facts.

## Unresolved questions
1. Phase 14 review score: 9.0 (file) or 9.6 (facts)?
2. Is `FSYNC_ENV=production` on cms-node, with dev mode off?
3. Are M1/M2 (core) and the phase 12 review-medium item closed?
4. Does the release pipeline enforce both relay URLs yet?
5. Transactional email provider: timeline before real users?
