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

## Progress (sync 2026-10-14, P0 Rust track on macOS)
- Complete: 0/16. Partial: 01, 02, 03, 06, 07, 08, 09. Scoped todos ticked: 28/37.
- Partial = code done for Mac-verifiable criteria. Open: two-machine measurements, Windows/C#, signing, procurement, team sign-off.
- Gate on final tree (2026-10-14 11:48): fmt, clippy `-D warnings`, workspace tests 253 pass / 0 fail, Swift 8/8, xcframework, e2e PASS. Earlier `devices_reset` PortNotExposed flake did not recur; root cause still open.
- **Open (P0 exit):** e2e recorded once on final tree, target 3 runs (relay-only stability). Mac-to-Windows run not done.
- Scope changes: own file/1 instead of iroh-blobs (ADR 007 addendum). History wrong-key no longer auto-resets (explicit `reset_history`). Re-pair after reset via `listed_head` (round 7). M5 TrustReset confirm API done (core, FFI, CLI; ADR 012). Not built: macOS app prompt UI (phase 11/12), email notice on reset. mdns-sd fallback deferred.
- Out of scope this sync (04, 05, 10-16): status unchanged, not synced.

## Phases
| # | Phase | Owner | Wk | Blocked by | Status |
|---|---|---|---|---|---|
| 01 | [Repo scaffold + decisions (backend lang, auth, min OS, certs)](phase-01-repo-scaffold-and-decisions.md) | B (+all) | P0 w1 | - | Partial |
| 02 | [Spike: iroh 1.x discovery/relay/blobs](phase-02-spike-iroh-discovery-relay.md) | A | P0 w1 | 01 | Partial |
| 03 | [Spike: FFI bindings Swift + C#; FFI contract](phase-03-spike-ffi-bindings.md) | A+D | P0 w2 | 01 | Partial (Swift done, C# open) |
| 04 | [Spike: macOS clipboard privacy + lazy file paste](phase-04-spike-macos-clipboard-lazy-paste.md) | C | P0 w1-3 | 01 | pending |
| 05 | [Spike: Windows clipboard + virtual-file lazy paste](phase-05-spike-windows-clipboard-lazy-paste.md) | D | P0 w1-3 | 01 | pending |
| 06 | [Core: identity, trust, transport, pairing](phase-06-core-identity-trust-transport.md) | A | P0 w2-3 | 02 | Partial |
| 07 | [Core: clip/1 + file/1 protocols](phase-07-core-protocols-clip-file.md) | A | P0 w3-4 | 06 | Partial |
| 08 | [Core: encrypted history store, FFI surface, CLI harness](phase-08-core-history-ffi-cli.md) | A | P0 w4-6 | 03, 07 | Partial |
| 09 | [Backend: auth, device registry, pairing codes, presence](phase-09-backend-auth-registry-presence.md) | B | P0 w1-4 | 01 | Partial |
| 10 | [Infra: relay + backend deploy, CI](phase-10-infra-relay-backend-deploy.md) | B | P0 w3-5 | 09 | pending |
| 11 | [macOS app: shell, auth, pairing, devices](phase-11-macos-app-shell-pairing.md) | C | P1 w1-5 | 03,04,08,09 | pending |
| 12 | [macOS app: clipboard, files, history, settings](phase-12-macos-clipboard-files-history.md) | C | P1 w5-11 | 11 | pending |
| 13 | [Windows app: shell, auth, pairing, devices](phase-13-windows-app-shell-pairing.md) | D | P1 w1-5 | 03,05,08,09 | pending |
| 14 | [Windows app: clipboard, files, history, settings](phase-14-windows-clipboard-files-history.md) | D | P1 w5-11 | 13 | pending |
| 15 | [Cross-OS integration + network test matrix](phase-15-integration-testing-cross-os.md) | B lead, all | P1 w2-13 | 10 (harness), 12,14 (full) | pending |
| 16 | [Release: signing, notarization, auto-update](phase-16-release-distribution.md) | B+C+D | P1 w8-14 | 10, 12, 14 | pending |

## Parallelism (no shared files across parallel phases)
- P0 w1: 01 first 2-3 days, then 02 ∥ 04 ∥ 05 ∥ 09.
- P0 w2-5: A runs 03→06→07→08; B runs 09→10; C/D finish spikes, start app skeletons against FFI contract (03).
- P1: 11∥13, then 12∥14; A = core hardening (owns `core/**`); B = 15 harness + 16 pipelines.
- Ownership: A `core/**`, `scripts/**`; B `backend/**`, `deploy/**`, `.github/**`, `tests/e2e/**`; C `apps/macos/**`; D `apps/windows/**`; `spikes/<name>/` per owner; `docs/**` via PR reviewed by lead.

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
2. Second relay region: US or EU (depends on target market).
3. Email provider for OTP (SES / Resend / Postmark). Blocks real OTP delivery (backend has `LogSender` only).
