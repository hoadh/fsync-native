---
title: "Four-track hardening round (A core, B Windows, C macOS, D infra)"
status: in-progress
created: 2026-10-15
parent: ../261013-2029-fsync-native-mvp/plan.md
base: main 2d87845
merged: local main 3c405b1 (not pushed)
---

# Goal
Close the open, agent-doable items of the MVP plan (see parent "Open, must close or be explicitly deferred").
Manual GUI QA, ARM64 hardware, LAN cabling (S7) and email-provider choice stay with the owner.

# Tracks
| Track | Branch | Owner paths | Items | Depends on |
|---|---|---|---|---|
| A core | `core/round-2` | `core/**`, `scripts/**` | A1 additive FFI history full-content accessor (`history_item_reps`), Swift bindings + xcframework, merge early. A2 ~30 s dial timeout after peer restart. A3 typed engine.lock error (FFI additive). A4 Postgres container-start flake + `logout_revocation` hang. A5 verify relay failover ("peer that cannot reach one relay uses the other"), test or fix | - |
| B Windows (waso) | `windows/round-2` | `apps/windows/**` | B1 S5 lows: PendingRevoke set only after Save, `WithoutAnEngineSignOutRevokesDirectly` flake (thread-safe list). B2 test for cancelling a started transfer. B3 phase 14 L1 (eager Task.Run), L2 (dispose message). B4 after A1/A3 merge: C# regen, DLL, history copy-back, typed lock error | A1, A3 for B4 |
| C macOS | `macos/round-2` | `apps/macos/**` | C1 S4 pending-revoke retry verified vs real backend (api-fsync.10z.one), automated where possible. C2 phase 12 review-medium item check. C3 after A1/A3 merge: history copy-back with full content, typed lock error. C4 manual QA checklist doc for owner (ADR 009 matrix) | A1, A3 for C3 |
| D infra | `infra/round-2` | `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**` | D1 first green GitHub CI run (Mac + Win jobs). D2 health alerts (backend, both relays, OTP cap reached). D3 rollback rehearsal on cms-node. D4 off-box Postgres backups (Ohio → SG). D5 SPF/DKIM/DMARC check, report only. D6 smtp_mailpit flake. D7 phase 15 harness skeleton on existing e2e | - |

# Rules
- Each track: own git worktree, own branch, only its owner paths. `plans/reports/` for reports. Lead edits plans/docs at merge.
- Gate per track: code-reviewer >= 9.5, 0 critical; tests 100%; core changes also need swift + mac integration + dotnet on waso.
- Windows runs go through Mac tmux session `fsync` (helpers `wrun.sh`/`wwait.sh` in lead scratchpad). Owner can watch with `tmux attach -t fsync`.
- Shared VPS hosts: never edit `/etc/caddy/Caddyfile` or reload Caddy; never print secrets; mind RAM.
- No force-push. Conventional commits, no AI references.

# Status
| Track | Status | Review | Merge |
|---|---|---|---|
| A | merged; open: A2 cached-conn fetch retry, A5 mid-run death + relay mesh | 9.5, 0 critical | 3c405b1, pushed f00980d |
| B | merged; open: ARM64, manual QA | 9.5 (B1-B4), B5 9.5 | 3c405b1, pushed f00980d |
| C | merged; open: manual QA run (C4), live-session revoke proof (C1) | 9.6, 0 critical/high | 3c405b1, pushed f00980d |
| D | merged; open: first green GitHub run on final main (billing), DKIM, backup key custody | 9.5, 0 critical/high/medium | 3c405b1, pushed f00980d |

Plan status: in-progress. Agent-doable items are done or partial as below. Closure needs: gates re-run after C merge, push, one green GitHub run, owner items.

# Item status
| Item | State | Evidence / still open |
|---|---|---|
| A1 history_item_reps | Done | baa46dd; used by C3 and B4. `docs/ffi-contract.md` entry open (lead) |
| A2 dial after restart | Done, gap | ~2 s redial (was 15-30 s); `tests/dial_after_restart.rs`. Gap: file/rep fetch on a dead cached conn has no retry |
| A3 typed engine.lock | Done | `EngineAlreadyRunning`, additive, contract v2; C# regen; Swift ErrorText. ffi-contract entry open |
| A4 timeouts and flakes | Done, partial root cause | Timeouts added. mailpit HTTP reset fixed. `logout_revocation` hang not reproduced (now fails loudly) |
| A5 relay failover | Partial | Connect-time failover verified. Mid-run relay death unverified. Peers on disjoint relays cannot connect |
| B1 S5 lows | Done | Pending revoke after Save; flake 15x green |
| B2 cancel started transfer | Done, fake engine | Pins app bookkeeping. Core partial-file cleanup e2e only |
| B3 phase 14 L1/L2 | Done | Eager Task.Run; Dispose order; IsDisposed guard |
| B4 copy-back + typed lock | Done | Full content via `HistoryItemReps`; waso dotnet 156/156 (lead) |
| B5 | Done per lead, scope unconfirmed | Eager zero-budget race fix; Windows CI green run 38636795483 (lead). Not in item list above: confirm |
| C1 pending revoke vs real backend | Partial | Gated test PASS vs staging (204, unreachable host). Live revoked session proven on local backend only |
| C2 phase 12 review-medium | Done | ca6d57d; phase 12 ticked |
| C3 copy-back full content | Done | Text/HTML/RTF/PNG; phase 12 ticked; 200-char deviation removed |
| C4 manual QA checklist | Doc done, not run | `plans/reports/qa-checklist-261015-macos-manual.md`. Owner |
| C5 Swift 6.1 CI fixes | Done locally | macOS last green on GitHub run 38638772422 (pre-refactor). Not re-run |
| D1 first green CI | Partial | Rust green on GitHub; Win and macOS green on different runs; no single green run on final main; billing blocks |
| D2 alerts | Implemented, unverified | Hourly probe workflow + `/health/otp` 503. Needs push and GitHub run; firing test not done |
| D3 rollback rehearsal | Done | ~88 s per hop. Incident: first attempt ~5 min staging 502; guard added |
| D4 off-box backups | Done | Nightly to SG, restore drilled. Failure visibility log-only until owner sets `BACKUP_PING_URL` |
| D5 SPF/DKIM/DMARC | Done, report only | SPF ok; DMARC p=none; DKIM not found. Owner action |
| D6 smtp_mailpit flake | Done | 8/8 and full workspace pass |
| D7 phase 15 harness skeleton | Partial | `run-matrix.sh` p0 PASS local and SG. netns N1-N9 not started |

# Scope changes (log)
- A4: `logout_revocation` fixed by timeouts (fail loud), not root cause. Reason: hang not reproduced in 12 runs. Impact: latent hang unverified.
- A5: mid-run relay-death test removed. Reason: in-process relays lack QUIC address discovery, iroh never re-homes (45 s). Impact: relay-death behaviour unverified. Relay mesh not in scope: documented limit.
- A2: file/rep fetch retry not done. Impact: open gap, decision needed.
- C1: live revoked-session proof on local backend only. Reason: deployed backend cannot issue a test session (OTP via Gmail, no test path). Impact: owner step.
- D2: alerting = hourly GitHub Actions probe with red-run notification, not a monitoring stack. Impact: depends on GitHub Actions, which is billing-blocked.
- D4: failure visibility = log only unless owner sets `BACKUP_PING_URL`.
- D3: rollback to a pre-migration release caused ~5 min api 502 on cms-node (rehearsal). Fixed by guard and `--force-rollback`.
- B2: cancel tests use a fake engine; real cancel is e2e only.

# Risks
| Risk | Level | Status |
|---|---|---|
| NEW: GitHub billing blocks CI; release gate not verifiable on final main | H | Open, owner |
| NEW: Backup private key only on Mac (`~/.fsync-backup`); losing it makes backups unreadable | H | Open, owner |
| NEW: Relays do not mesh; peers on disjoint relays cannot connect | H | Open, decision |
| cms-node ran branch build 549cc6a3d298, not main | M | Closed: redeployed main f00980db3978 |
| NEW: Local main 3c405b1 not pushed; divergence risk | M | Open, lead |
| NEW: No DKIM; OTP deliverability at risk | M | Open, owner |
| NEW: Staging 502 (D rehearsal, 03:08-03:13 local) and C-observed 502 (03:10-03:12 +07) likely the same | L | Unconfirmed |
| Carried: mid-run relay death unverified | M | Open |
| Carried: Postgres TLS not enabled (before remote DB) | M | Open |
| Carried: ~600 MiB RAM free on shared host | M | Open |
| Closed: 30 s dial after restart (A2) | - | Closed, ~2 s measured |
| Closed: smtp_mailpit flake (D6, A4) | - | Closed |
| Closed: Win `WithoutAnEngineSignOut` flake (B1) | - | Closed |
| Closed: string-matched engine.lock error (B4) | - | Closed |
| Closed: history copy-back blocked on FFI (A1, B4, C3) | - | Closed |

# Next actions (owner, definition of done)
1. DONE (cargo 308/0, swift 222/0 fail, mac-it PASS; dotnet 156/156 on A+B+D, C is macOS-only). Lead: re-run gates on 3c405b1 after C merge (cargo, fmt/clippy, swift, mac integration, dotnet on waso). DoD: all green; numbers in MVP plan Progress.
2. DONE (2d87845..f00980d; CI run 38662663286 not started: billing). Lead: push main, no force-push. DoD: origin/main = local main; CI dispatched.
3. Owner: restore GitHub billing and re-dispatch CI on pushed main. DoD: one run with rust, macOS and Windows green; then tick phase 10 CI and verify D2.
4. DONE (f00980db3978, public smoke + probe-health ok). Lead or D: redeploy cms-node from main, drop branch build. DoD: `/health` ok at main sha; `deploy/operations.md` followed.
5. Owner: copy `~/.fsync-backup/fsync-backup-secret.asc` to password manager. DoD: owner confirms entry exists.
6. Owner: enable DKIM in Google Admin, publish `google._domainkey` TXT. DoD: record resolves; then decide DMARC tightening.
7. Owner: pick `BACKUP_PING_URL` or accept log-only. DoD: decision written in phase 10.
8. Owner: run manual QA (macOS checklist; Windows phase 14 list); ARM64 run. DoD: Result column filled; failed rows filed to owners.
9. Owner, optional: `FSYNC_REAL_REFRESH` live revoke run. DoD: `RealBackendRevokeTests` passes with live token, or sign-off to defer.
10. Track A, or owner sign-off to defer: file/rep dead-cached-conn retry; A5 mid-run death on real relays; relay mesh decision. DoD: test added, or deferral signed off in MVP plan.
11. DONE (415904a). Lead: `docs/ffi-contract.md` "Additive since v2" lists `history_item_reps` and `EngineAlreadyRunning`; roadmap and changelog updated. DoD: entries present; docs impact stated.
12. Backend owner (MVP role B): 10k WS load test, Postgres TLS before remote DB, phase 15 netns N1/N2/N5. DoD: each green or deferred with sign-off.
13. Owner: S7 LAN cabling, or sign off Tailscale-only. DoD: decision recorded in MVP plan.

# Unresolved questions
1. B5 (answered): eager zero-budget race fix in Windows ClipboardService (CI "engine not started"), regression test added, CI windows job green run 38636795483.
2. Is the C-observed 502 (03:10-03:12 +07) the same incident as the D rehearsal outage? Likely; unconfirmed.
3. Is `FSYNC_ENV=production` with dev mode off on cms-node? Still unverified.
4. Postgres TLS: no evidence this round. Status?
5. Relay mesh: accept "peers must share a relay" or fix in core?
6. Windows manual QA: is a Windows checklist file needed? Only the macOS one exists.
7. Must file/rep fetch retry on a dead cached connection land before release?
