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

## Progress (sync 2026-10-15, main 461dd57 on origin)
- Done 10-15: S1 Gmail SMTP OTP sender + 450/24 h cap (real OTP in inbox). S2 relay-only mode, SG relay live. S3/S4 core `relay_urls` + `revoke_session`, macOS pending revoke (bb412e8). S5 Windows wiring (7cc8cfd). S6 two-relay e2e PASS. Windows phase 14 merged (37e0430). Detail: [SG/revoke plan](../261014-2314-smtp-sg-relay-revoke-session/plan.md).
- Gates on main: cargo 300/0; fmt+clippy clean; swift FsyncMac 208 (14 integration skipped) 0 fail; mac integration PASS; xcframework rebuilt; dotnet 144/144 (waso).
- Blocked: S7 LAN (owner cabling). Tailscale = accepted fallback.
- Phases: done 11 (Mac), 14 (done w/ concerns); partial 01-10, 12, 13; pending 15, 16.
- Scope changes: Gmail SMTP as OTP sender (transactional provider still advised before real traffic); SG chosen as 2nd relay; `revoke_session` pulled into MVP (was deferred); release builds must pass both relay URLs.
- Open, must close or be explicitly deferred:
  - Manual QA: Mac+Win GUI matrices (04, 12, 13, 14), Mac↔Win pairing via GUI, privacy alert, password managers.
  - LAN re-test (S7, owner cabling).
  - Infra: first GitHub CI run (Win job unverified); monitoring/alerts; rollback rehearsal; off-box backups + snapshots; staging/prod split; backend-deploy workflow; RAM (~600 MiB free on shared host).
  - Backend: 10k WS load test; Postgres TLS before remote DB; SPF/DKIM/DMARC status unverified.
  - Core (A): history full-content accessor (blocks copy-back on Mac 12, Win 14); ~30 s dial timeout after peer restart; typed engine.lock error. (M1/M2 + phase 12 review medium fixed in ca6d57d.)
  - Apps: ARM64 hardware (03, 13, 14); phase 12 review-medium item unchecked in file; phase 11 Low items (Q7).
  - Release 16: signed launch-at-login check. Phase 15 harness not built.
  - Flakes: backend smtp_mailpit (1 fail under load, 8/8 on rerun); Postgres container start (core); Win `WithoutAnEngineSignOutRevokesDirectly` (review L2).
- Next (owner → done when): A dial fix, history accessor, tests green · C manual matrix in ADR 009, S4 retry verified · D Win GUI run, ARM64 build run, alerts, rollback rehearsal · B first green CI run, 15 harness, 16 · S7 LAN re-test when cabled, or owner signs off Tailscale-only · lead: push after review. **Finish the plan: every open item must close or be explicitly deferred with owner sign-off.**

## Phases
| # | Phase | Owner | Wk | Blocked by | Status |
|---|---|---|---|---|---|
| 01 | [Repo scaffold + decisions](phase-01-repo-scaffold-and-decisions.md) | B (+all) | P0 w1 | - | Partial |
| 02 | [Spike: iroh discovery/relay/blobs](phase-02-spike-iroh-discovery-relay.md) | A | P0 w1 | 01 | Partial |
| 03 | [Spike: FFI bindings Swift + C#; contract](phase-03-spike-ffi-bindings.md) | A+D | P0 w2 | 01 | Partial (Swift+C# done; ARM64 not run; additive relay_urls, revoke_session) |
| 04 | [Spike: macOS clipboard + lazy file paste](phase-04-spike-macos-clipboard-lazy-paste.md) | C | P0 w1-3 | 01 | Partial (automated; ADR 009; manual open) |
| 05 | [Spike: Windows clipboard + virtual-file paste](phase-05-spike-windows-clipboard-lazy-paste.md) | D | P0 w1-3 | 01 | Partial (automated; ADR 010) |
| 06 | [Core: identity, trust, transport, pairing](phase-06-core-identity-trust-transport.md) | A | P0 w2-3 | 02 | Partial |
| 07 | [Core: clip/1 + file/1](phase-07-core-protocols-clip-file.md) | A | P0 w3-4 | 06 | Partial (read-ahead merged; LAN measures open) |
| 08 | [Core: history, FFI, CLI harness](phase-08-core-history-ffi-cli.md) | A | P0 w4-6 | 03, 07 | Partial (P0 exit demo open) |
| 09 | [Backend: auth, registry, presence](phase-09-backend-auth-registry-presence.md) | B | P0 w1-4 | 01 | Partial (Gmail SMTP live; 10k WS load test open) |
| 10 | [Infra: relay + backend deploy, CI](phase-10-infra-relay-backend-deploy.md) | B | P0 w3-5 | 09 | Partial (2 relays live incl. SG; alerts, rollback, CI on GitHub open) |
| 11 | [macOS app: shell, auth, pairing, devices](phase-11-macos-app-shell-pairing.md) | C | P1 w1-5 | 03,04,08,09 | Done on Mac (manual visual checks open) |
| 12 | [macOS app: clipboard, files, history](phase-12-macos-clipboard-files-history.md) | C | P1 w5-11 | 11 | Partial (automated done; manual QA open) |
| 13 | [Windows app: shell, auth, pairing](phase-13-windows-app-shell-pairing.md) | D | P1 w1-5 | 03,05,08,09 | Partial (GUI run, LAN, ARM64 open) |
| 14 | [Windows app: clipboard, files, history](phase-14-windows-clipboard-files-history.md) | D | P1 w5-11 | 13 | Done w/ concerns (merged 37e0430; manual QA, ARM64 open) |
| 15 | [Cross-OS integration + network test matrix](phase-15-integration-testing-cross-os.md) | B lead, all | P1 w2-13 | 10, 12, 14 | Pending (ad hoc runs only) |
| 16 | [Release: signing, notarization, auto-update](phase-16-release-distribution.md) | B+C+D | P1 w8-14 | 10, 12, 14 | Pending (Developer ID + notarization done) |

## Ownership and merge rules
- Ownership: A `core/**`, `scripts/**`; B `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**`; C `apps/macos/**`; D `apps/windows/**`; `spikes/<name>/` per owner; `docs/**` via PR reviewed by lead.
- Shared docs and this plan edited only by lead at merge. Merge: review >= 9.5; cargo + swift tests (+ dotnet for core changes).
- Sync via GitHub `origin` only; never force-push.
- Critical path: 01 → 02 → 06 → 07 → 08 → 11/13 → 12/14 → 15 → 16.

## Global rules
- Code files < 200 lines. Wire formats versioned by ALPN (`fsync/clip/1`, `fsync/file/1`, `fsync/pair/1`); never mutate v1, add v2.
- No plaintext clipboard/file bytes touch backend or relay.

## Future (no phase files)
- P2: per-device targeting, folders/very large transfers, UX polish, OAuth, recovery key, relay usage caps.
- P3: billing, plan-tier relay caps, teams/orgs, Linux/iOS/Android.

## Resolved decisions (validation S1)
Auth = own email OTP · Min OS = macOS 13+, Win 10 22H2+/11 · Recovery = trust reset + re-pair · Telemetry = opt-in crash only · History encrypted at rest · Relay = 2 regions (SG + US/EU; SG live, US/EU = Ohio relay-fsync.10z.one) · Limits = defaults · Windows UI = WinUI 3

## Unresolved questions
1. Pricing per tier (limits configurable, not tied to tiers in MVP). Open.
2. ~~Second relay region~~ Resolved 2026-10-14: Singapore (Lightsail, relay-only), deployed 2026-10-15 at relay-sg-fsync.10z.one, e2e PASS (S6).
3. ~~Email provider for OTP~~ Resolved 2026-10-15: Gmail SMTP (Workspace sender), cap 450/24 h, real OTP inbox confirmed. Follow-up: transactional provider before real traffic.
4. Developer ID signing built (team VE5P5RS83W), notarization verified. Open: signed launch-at-login check (phase 16).
5. ~~Offline `logout()`~~ Resolved 2026-10-15: additive `revoke_session` added (S3/S4); pending revoke on quit on macOS (bb412e8) and Windows (7cc8cfd).
6. macOS integration script not in CI; CI workflows never run on GitHub yet (phase 10).
7. Optional Low items from phase 11 review round 3 (LoginModel waits for pending clear; boot awaits `sessionClear`).
8. Mac↔Win LAN unreachable (separate ISP routers). Deferred by owner (S7 blocked on cabling). Tailscale accepted as fallback until then.
