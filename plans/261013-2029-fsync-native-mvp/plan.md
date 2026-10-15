---
title: "fsync-native MVP (P0 foundation + P1 macOS/Windows apps)"
description: "Rust core (iroh) + SwiftUI/WinUI 3 apps for E2E clipboard & file sync, LAN-first with relay fallback."
status: in-progress
priority: P1
effort: ~17-19 wk calendar (P0 4-5 wk + P1 11-14 wk), team of 4
branch: main
tags: [rust, iroh, uniffi, swiftui, winui3, clipboard, p2p, backend]
created: 2026-10-13
updated: 2026-10-15
blockedBy: []
blocks: []
---

# fsync-native MVP

Source: [brainstorm report](../reports/brainstorm-261013-2029-cross-platform-clipboard-file-sync-design.md) · [Validation log](validation-log.md)
Specs: layout+ADRs [01](phase-01-repo-scaffold-and-decisions.md) · DeviceList+pair/1 [06](phase-06-core-identity-trust-transport.md) · clip/1+file/1 [07](phase-07-core-protocols-clip-file.md) · FFI [03](phase-03-spike-ffi-bindings.md) · API [09](phase-09-backend-auth-registry-presence.md) · test matrix [15](phase-15-integration-testing-cross-os.md)
Roles: A = Rust core | B = backend+DevOps | C = macOS | D = Windows

## Progress (sync 2026-10-15, main e536ce7, pushed)
- Round 2 hardening merged to local main 3c405b1: [four-track plan](../261015-0410-four-track-hardening/plan.md). Closure needs: gates re-run after C, push, one green GitHub CI run, owner items.
- Done 10-15: S1 Gmail SMTP OTP (450/24 h cap); S2 SG relay-only, S6 two-relay e2e PASS; S3/S4 `revoke_session` + pending revoke (mac bb412e8, Win 7cc8cfd); Win phase 14 merged (37e0430). Round 2 landed: A1-A5 core (A2 gap, A5 partial), B1-B5 Windows, C1-C5 macOS (C1 partial), D1-D7 infra (D1/D2 unverified on GitHub).
- Gates on merged main, pre-C: cargo 308/0; fmt+clippy clean; swift 208 (14 skipped) 0 fail; mac integration PASS; dotnet waso 156/156. Track C alone: swift 222 (18 skipped) 0 fail. Post-C gates: lead re-running.
- S7 LAN done 10-15: Mac↔Win same LAN verified; core now LAN-first (ccc1a31, generic interface rule, no Tailscale dependency); Mac history shows device names (2d3d2d5). Blocked: GitHub Actions billing (owner).
- Phases: done 11 (Mac), 14 (w/ concerns); partial 01-10, 12, 13; in progress 15 (harness skeleton); pending 16.
- Scope changes: Gmail SMTP as OTP sender (transactional provider still advised); SG second relay; `revoke_session` in MVP; release builds pass both relay URLs; relays do not mesh (documented limit); A4 logout hang fail-loud, root cause not reproduced; A5 mid-run relay test dropped; D2 alerts = GitHub Actions probe, not a monitoring stack.
- Open, must close or be explicitly deferred with owner sign-off:
  - Manual QA: macOS checklist (`plans/reports/qa-checklist-261015-macos-manual.md`); Win GUI matrix (14); Mac↔Win GUI pairing; privacy alert; password managers.
  - ARM64 hardware (03, 13, 14). UI "On LAN" label still means any direct path.
  - Infra: first green GitHub CI on final main; verify alerts after push; `BACKUP_PING_URL` decision; backup key to password manager; staging/prod split; backend-deploy workflow; RAM (~600 MiB free).
  - Email: DKIM not found (SPF ok, DMARC p=none). Owner enables in Google Admin.
  - Backend: 10k WS load test; Postgres TLS before remote DB.
  - Core (A): file/rep fetch retry on dead cached conn; A5 mid-run relay death; relay-mesh decision; `docs/ffi-contract.md` additive entries (lead); M1/M2 engine.lock (phase 08).
  - Apps/release: phase 11 Low items (Q7); C1 live-session revoke proof (owner `FSYNC_REAL_REFRESH`, optional); phase 16 signed launch-at-login check.
  - Phase 15: netns N1-N9, security cases, cross-OS runners, metrics (not started).
  - Closed 10-15: smtp_mailpit flake (D6); Win `WithoutAnEngineSignOut` flake (B1); ~30 s dial after restart (A2); history copy-back blocked on FFI (A1, B4, C3); rollback rehearsal (D3); off-box backups (D4).
- Next: four-track plan "Next actions" (13 items, owners, definition of done). **Finish the plan: every open item must close or be explicitly deferred with owner sign-off.**

## Phases
| # | Phase | Owner | Wk | Blocked by | Status |
|---|---|---|---|---|---|
| 01 | [Repo scaffold + decisions](phase-01-repo-scaffold-and-decisions.md) | B (+all) | P0 w1 | - | Partial |
| 02 | [Spike: iroh discovery/relay/blobs](phase-02-spike-iroh-discovery-relay.md) | A | P0 w1 | 01 | Partial |
| 03 | [Spike: FFI bindings Swift + C#; contract](phase-03-spike-ffi-bindings.md) | A+D | P0 w2 | 01 | Partial (Swift+C# done; ARM64 not run; additive relay_urls, revoke_session, history_item_reps, EngineAlreadyRunning) |
| 04 | [Spike: macOS clipboard + lazy file paste](phase-04-spike-macos-clipboard-lazy-paste.md) | C | P0 w1-3 | 01 | Partial (automated; ADR 009; manual open) |
| 05 | [Spike: Windows clipboard + virtual-file paste](phase-05-spike-windows-clipboard-lazy-paste.md) | D | P0 w1-3 | 01 | Partial (automated; ADR 010) |
| 06 | [Core: identity, trust, transport, pairing](phase-06-core-identity-trust-transport.md) | A | P0 w2-3 | 02 | Partial |
| 07 | [Core: clip/1 + file/1](phase-07-core-protocols-clip-file.md) | A | P0 w3-4 | 06 | Partial (read-ahead merged; LAN measures open) |
| 08 | [Core: history, FFI, CLI harness](phase-08-core-history-ffi-cli.md) | A | P0 w4-6 | 03, 07 | Partial (round 2 A1-A5; P0 exit demo open) |
| 09 | [Backend: auth, registry, presence](phase-09-backend-auth-registry-presence.md) | B | P0 w1-4 | 01 | Partial (Gmail SMTP live; 10k WS load test open) |
| 10 | [Infra: relay + backend deploy, CI](phase-10-infra-relay-backend-deploy.md) | B | P0 w3-5 | 09 | Partial (2 relays live; rollback rehearsed, off-box backups live; alerts unverified on GitHub; prod split open) |
| 11 | [macOS app: shell, auth, pairing, devices](phase-11-macos-app-shell-pairing.md) | C | P1 w1-5 | 03,04,08,09 | Done on Mac (manual visual checks open) |
| 12 | [macOS app: clipboard, files, history](phase-12-macos-clipboard-files-history.md) | C | P1 w5-11 | 11 | Partial (automated, copy-back done; manual QA open) |
| 13 | [Windows app: shell, auth, pairing](phase-13-windows-app-shell-pairing.md) | D | P1 w1-5 | 03,05,08,09 | Partial (GUI run, LAN, ARM64 open) |
| 14 | [Windows app: clipboard, files, history](phase-14-windows-clipboard-files-history.md) | D | P1 w5-11 | 13 | Done w/ concerns (merged 37e0430; round 2 on main; manual QA, ARM64 open) |
| 15 | [Cross-OS integration + network test matrix](phase-15-integration-testing-cross-os.md) | B lead, all | P1 w2-13 | 10, 12, 14 | In progress (D7 harness skeleton; netns not started) |
| 16 | [Release: signing, notarization, auto-update](phase-16-release-distribution.md) | B+C+D | P1 w8-14 | 10, 12, 14 | Pending (Developer ID + notarization done) |

## Ownership and merge rules
- Ownership: A `core/**`, `scripts/**`; B `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**`; C `apps/macos/**`; D `apps/windows/**`; `spikes/<name>/` per owner; `docs/**` via PR reviewed by lead.
- Shared docs and this plan edited only by lead at merge. Merge: review >= 9.5; cargo + swift tests (+ dotnet for core changes).
- Sync via GitHub `origin` only; never force-push.
- Critical path: 01 → 02 → 06 → 07 → 08 → 11/13 → 12/14 → 15 → 16.

## Global rules
- Code files < 200 lines. Wire formats versioned by ALPN (`fsync/clip/1`, `fsync/file/1`, `fsync/pair/1`); never mutate v1, add v2.
- No plaintext clipboard/file bytes touch backend or relay.

## Future and resolved (no phase files)
- Future: P2 per-device targeting, folders/large transfers, UX polish, OAuth, recovery key, relay caps; P3 billing, plan-tier caps, teams/orgs, Linux/iOS/Android.
- Resolved (S1): own email OTP · macOS 13+, Win 10 22H2+/11 · trust reset + re-pair · opt-in crash telemetry · history encrypted at rest · 2 relay regions (SG + US/EU) · default limits · WinUI 3.

## Unresolved questions
1. Pricing per tier (limits configurable, not tied to tiers in MVP).
2. Transactional email provider before real traffic (Gmail SMTP is interim).
3. Signed launch-at-login check (phase 16); macOS integration script not in CI (phase 10).
4. Mac↔Win LAN unreachable; Tailscale fallback until S7 (owner).
5. Seven open technical questions are in the four-track plan (B5 scope, relay mesh, 502 incident link, FSYNC_ENV on cms-node, Postgres TLS, Windows QA doc, dead-cached-conn retry).
