---
title: "fsync-native MVP (P0 foundation + P1 macOS/Windows apps)"
description: "Rust core (iroh) + SwiftUI/WinUI 3 apps for E2E clipboard & file sync, LAN-first with relay fallback."
status: in-progress
priority: P1
effort: ~17-19 wk calendar (P0 4-5 wk + P1 11-14 wk), team of 4
branch: main
tags: [rust, iroh, uniffi, swiftui, winui3, clipboard, p2p, backend]
created: 2026-10-13
blockedBy: []
blocks: []
---

# fsync-native MVP

Source: [brainstorm report](../reports/brainstorm-261013-2029-cross-platform-clipboard-file-sync-design.md) · [Validation log](validation-log.md)
Specs: layout+ADRs [01](phase-01-repo-scaffold-and-decisions.md) · DeviceList+pair/1 [06](phase-06-core-identity-trust-transport.md) · clip/1+file/1 [07](phase-07-core-protocols-clip-file.md) · FFI contract [03](phase-03-spike-ffi-bindings.md) · API [09](phase-09-backend-auth-registry-presence.md) · test matrix [15](phase-15-integration-testing-cross-os.md)

## Roles
A = Rust core | B = backend+DevOps | C = macOS | D = Windows

## Progress (sync 2026-10-14 night, tracks A/C/D/X merged)
- Merged to local main d3435e8, not pushed. Gates: cargo 264/264, clippy+fmt clean; Swift CoreKit 8, FsyncMac 188 (12 integration skipped), integration 12/12; P0 e2e PASS (direct 256 MiB 35.6 MiB/s, relay-only 49.7 MiB/s); dotnet 44/44; Windows cargo 173 pass, 17 fails are Docker-only (env).
- Complete: 0/16 (11 done on Mac, manual checks open). Partial: 01-10, 12, 13. Pending: 14 (Track B running, untouched), 15, 16.
- C core hardening (review 9.6, 0 critical): read-ahead ADR 010 (50 ms RTT 46→105 MiB/s); logout revokes refresh token (3 s inline, bg retry, family revoke); `engine.lock`; devices_reset flake fixed; e2e 3/3.
- D infra (review 9.6): shared VPS cms-node, api-fsync/relay-fsync.10z.one with valid TLS, real client IP via Caddy XFF, nightly backup + restore script. Staging: OTP to log, no provider. CI never run on GitHub.
- D public two-client e2e: pair (SAS), clip direct 22 ms / relay 802 ms, 16 MiB file direct+relay hash OK. Relay 0.105 MiB/s = raw VPS path (network-bound, not code).
- A macOS 04+12 (review 9.5): ADR 009 hybrid (eager <20 MiB, lazy larger); NSFilePromiseProvider no-go. Phase 12 implemented, automated only.
- X Mac↔Win (fsync-cli on both sides) over Tailscale PASS: code+SAS, clip both ways direct+relay, 64 MiB both ways hash OK, remove/re-pair. Win GUI not run; Mac↔Win LAN unreachable. Report: plans/reports/tester-261014-2157-mac-windows-pairing.md.
- Scope changes: own file/1 not iroh-blobs (ADR 007 addendum); wrong-key history no longer auto-resets (explicit `reset_history`); history copy-back = 200-char text preview only (FFI gap); folders out of MVP; share extension dropped (YAGNI); M5 trust-reset API done (ADR 012); email notice on reset not built.
- Open core: M1 (unbounded lingering-engine wait holds engine.lock if a handle leaks), M2 (stop() waits 5 s vs 3 s teardown); additive FFI `revoke_session` deferred (user decision).
- Open apps: 04 manual paste matrix, password managers, privacy alert; 12 manual QA, full-content history copy-back, review medium (sync never starts after failed initial settings load); 13 Win GUI run, LAN path, ARM64 hardware.
- Open infra: email provider, monitoring/alerts, rollback rehearsal, 2nd relay region, off-box backups + Lightsail snapshots, RAM (~600 MiB free on shared host).
- Next (owner → done when): A M1/M2 fixed, tests green · C 04/12 manual matrix recorded in ADR 009 · D email sender, alerts, rollback rehearsal · B finish 14, then 15 harness · lead: push after review. **Finish the plan: every open item above must close or be explicitly deferred.**

## Phases
| # | Phase | Owner | Wk | Blocked by | Status |
|---|---|---|---|---|---|
| 01 | [Repo scaffold + decisions (backend lang, auth, min OS, certs)](phase-01-repo-scaffold-and-decisions.md) | B (+all) | P0 w1 | - | Partial |
| 02 | [Spike: iroh 1.x discovery/relay/blobs](phase-02-spike-iroh-discovery-relay.md) | A | P0 w1 | 01 | Partial |
| 03 | [Spike: FFI bindings Swift + C#; FFI contract](phase-03-spike-ffi-bindings.md) | A+D | P0 w2 | 01 | Partial (Swift + C# done; ARM64 not executed) |
| 04 | [Spike: macOS clipboard privacy + lazy file paste](phase-04-spike-macos-clipboard-lazy-paste.md) | C | P0 w1-3 | 01 | Partial (automated done, ADR 009; manual open) |
| 05 | [Spike: Windows clipboard + virtual-file lazy paste](phase-05-spike-windows-clipboard-lazy-paste.md) | D | P0 w1-3 | 01 | Partial (automated done, ADR 010) |
| 06 | [Core: identity, trust, transport, pairing](phase-06-core-identity-trust-transport.md) | A | P0 w2-3 | 02 | Partial |
| 07 | [Core: clip/1 + file/1 protocols](phase-07-core-protocols-clip-file.md) | A | P0 w3-4 | 06 | Partial (read-ahead merged; LAN measures open) |
| 08 | [Core: encrypted history store, FFI surface, CLI harness](phase-08-core-history-ffi-cli.md) | A | P0 w4-6 | 03, 07 | Partial (P0 exit demo open) |
| 09 | [Backend: auth, device registry, pairing codes, presence](phase-09-backend-auth-registry-presence.md) | B | P0 w1-4 | 01 | Partial (deployed to staging VPS; email open) |
| 10 | [Infra: relay + backend deploy, CI](phase-10-infra-relay-backend-deploy.md) | B | P0 w3-5 | 09 | Partial (staging live, public TLS; alerts, rollback, 2nd region open) |
| 11 | [macOS app: shell, auth, pairing, devices](phase-11-macos-app-shell-pairing.md) | C | P1 w1-5 | 03,04,08,09 | Done on Mac (manual visual checks open; Mac↔Win CLI pairing PASS) |
| 12 | [macOS app: clipboard, files, history, settings](phase-12-macos-clipboard-files-history.md) | C | P1 w5-11 | 11 | Partial (automated done; manual QA open) |
| 13 | [Windows app: shell, auth, pairing, devices](phase-13-windows-app-shell-pairing.md) | D | P1 w1-5 | 03,05,08,09 | Partial (e2e + Mac↔Win pairing PASS; GUI run, ARM64 open) |
| 14 | [Windows app: clipboard, files, history, settings](phase-14-windows-clipboard-files-history.md) | D | P1 w5-11 | 13 | pending (Track B running) |
| 15 | [Cross-OS integration + network test matrix](phase-15-integration-testing-cross-os.md) | B lead, all | P1 w2-13 | 10 (harness), 12,14 (full) | pending (ad hoc runs only) |
| 16 | [Release: signing, notarization, auto-update](phase-16-release-distribution.md) | B+C+D | P1 w8-14 | 10, 12, 14 | pending |

## Parallelism (no shared files across parallel phases)
- P0 w1: 01 first 2-3 days, then 02 ∥ 04 ∥ 05 ∥ 09.
- P0 w2-5: A runs 03→06→07→08; B runs 09→10; C/D finish spikes, start app skeletons against FFI contract (03).
- P1: 11∥13, then 12∥14; A = core hardening (owns `core/**`); B = 15 harness + 16 pipelines.
- Ownership: A `core/**`, `scripts/**`; B `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**`; C `apps/macos/**`; D `apps/windows/**`; `spikes/<name>/` per owner; `docs/**` via PR reviewed by lead.

## Execution tracks (2 machines, from 2026-10-14)
Mac = macOS repo (lead/orchestrator). Windows = `ssh dev@waso`, repo `C:\Users\dev\workspace\fsync-native`, Claude session `c8122a94-d7e4-4edd-b247-a823d18c797d` (resumed headless via SSH). Sync only via GitHub `origin`; never force-push.

| Track | Machine / branch | Work | Owns | Waits on |
|---|---|---|---|---|
| A macOS | Mac worktree `macos/phase-04-12` (merged) | 04 → 12 | `apps/macos/**`, `spikes/macos-*` | - |
| B Windows | Windows `windows/phase-14` (running) | 14 | `apps/windows/**`, `spikes/windows-*` | - |
| C Core | Mac worktree `core/hardening` (merged) | read-ahead, logout revoke, lock, flake, e2e x3 | `core/**`, `scripts/**` | - |
| D Infra | Mac worktree `infra/phase-10` (merged) | 10 on AWS Lightsail VPS | `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**` | - |
| X Cross-OS | Mac + Windows (done, Tailscale) | Mac ↔ Windows pairing/transfer | reports only | - |

Rules:
- A track edits only its own paths; anything else goes to the owner or a PR the owner reviews.
- Shared docs (`docs/project-changelog.md`, `docs/codebase-summary.md`, `docs/system-architecture.md`, this `plan.md`) are edited only by the lead when merging a track. Tracks update their own phase file and write `plans/reports/`.
- Merge to main: lead runs cargo + swift tests (+ `dotnet test` on Windows for core changes); review >= 9.5.
- 15 starts when 10 provides the harness; full 15 and 16 after 12 + 14.

## Critical path
01 → 02 → 06 → 07 → 08 → 11/13 → 12/14 → 15 → 16. Spikes 04/05 gate 12/14 (lazy paste vs eager fallback).

## Global rules
- Code files < 200 lines; split modules by concern (see phase-01 layout).
- Wire formats versioned by ALPN (`fsync/clip/1`, `fsync/file/1`, `fsync/pair/1`); never mutate v1, add v2.
- No plaintext clipboard/file bytes ever touch backend or relay (relay sees QUIC-encrypted only).

## Future (no phase files)
- P2: per-device targeting, folder/very large transfers, UX polish, OAuth providers, recovery key, relay usage caps.
- P3: billing, plan-tier relay caps, teams/orgs, Linux/iOS/Android.

## Resolved decisions (validation S1)
Auth = own email OTP · Min OS = macOS 13+, Win 10 22H2+/11 · Recovery = trust reset + re-pair · Telemetry = opt-in crash only · History encrypted at rest · Relay = 2 regions (SG + US/EU) · Limits = defaults · Windows UI = WinUI 3

## Unresolved questions
1. Pricing per tier (limits configurable, not tied to tiers in MVP).
2. Second relay region: SG vs US/EU (depends on target market). Data: Track D from Vietnam to US-East-2 VPS, raw path 0.11 MiB/s = relay 0.105; nearer region is the lever. Supports SG.
3. ~~Email provider for OTP~~ Resolved 2026-10-15: Gmail SMTP (Workspace sender), global cap 450/24 h; real OTP delivery confirmed by owner (inbox). Transactional provider still advised before real traffic.
4. Developer ID signing: decided and built (team VE5P5RS83W); notarization verified. Open: signed launch-at-login check (phase 16).
5. Offline `logout()`: partly resolved (inline + bg revoke, review 9.6). Open: quit-before-revoke leaves refresh token valid to 30 d expiry. Decide: add additive FFI `revoke_session` now, or accept for MVP.
6. macOS integration script not in CI (phase 10 to wire). CI workflows never run on GitHub yet.
7. Optional Low items from phase 11 review round 3 (LoginModel waits for pending clear; boot awaits `sessionClear`).
8. Mac↔Win LAN unreachable (separate ISP routers). Decided: put both on one router and re-test over LAN (deferred by owner, later); Tailscale accepted when LAN is not available.
