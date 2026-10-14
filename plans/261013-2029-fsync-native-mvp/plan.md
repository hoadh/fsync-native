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
- Scope changes: own file/1 instead of iroh-blobs (ADR 007 addendum). History wrong-key no longer auto-resets (explicit `reset_history`). Re-pair after reset via `listed_head` (round 7). M5 TrustReset confirm API done (core, FFI, CLI; ADR 012). macOS reset prompt UI built as library `TrustResetPrompt` (plan 261014-1206), wiring in phase 11. Not built: email notice on reset. mdns-sd fallback deferred.
- Phase 11 (macOS app shell): Done on Mac, manual visual checks open. Plan: [macos-app-shell-pairing](261014-1235-macos-app-shell-pairing/plan.md). Review 9.0 → 9.5 → 9.6 (0 critical). FsyncMac 89 tests (3 integration skipped by default), CoreKit 8+14, integration PASS, Cargo 253 passed / 1 ignored. Bundle ad-hoc signed, runtime. Deviations in phase 11 file.
- Phase 12 note: Lan/Relay status needs live connections (phase 11 shows "Online" until sync connects peers). A signed build should verify the Keychain migration.
- Out of scope this sync (04, 05, 10, 12-16): status unchanged, not synced.

## Progress (sync 2026-10-14, Windows track on Win11 x64, branch windows/phase-03-13)
- 03 C#: uniffi-bindgen-cs v0.10.0+v0.29.4 works on contract v2; x64 DLL; 9/9 C# contract tests; 10k-callback stress OK; 10 MiB round trip 27 ms (misses 20 ms like Swift; bulk by path/chunks). ADR 008 decides bindgen-cs (no C ABI). ARM64 DLL builds (clang); not executed on ARM64 hardware.
- 05: automated self-test all pass (filter, listener, cross-process read, shell paste 3 files + 200 MiB, failure mid-file, exit policy) + WinUI 3 tray smoke. ADR 010: lazy paste via virtual files; clear on exit; core needs read-ahead in `read_file_range` (shell reads 256 KiB serially). Open: manual app matrix, real password managers, Win10, ARM64 hardware.
- 13: solution + WinUI 3 tray app (single instance, DPAPI secrets, onboarding, pairing code/QR/SAS, devices + remove, start with Windows); 35/35 view-model tests; e2e `apps/windows/tests/e2e/pair-with-cli.ps1` PASS (OTP sign-in, code + SAS pairing with fsync-cli, live presence status, clip delivered to the app engine, remove, startup toggle). Found + fixed a core Windows bug (blob `finish` fsync on a read-only handle: Access denied); Rust workspace on Windows 251 pass / 0 fail. Open: pairing with Mac, ARM64 hardware.

## Phases
| # | Phase | Owner | Wk | Blocked by | Status |
|---|---|---|---|---|---|
| 01 | [Repo scaffold + decisions (backend lang, auth, min OS, certs)](phase-01-repo-scaffold-and-decisions.md) | B (+all) | P0 w1 | - | Partial |
| 02 | [Spike: iroh 1.x discovery/relay/blobs](phase-02-spike-iroh-discovery-relay.md) | A | P0 w1 | 01 | Partial |
| 03 | [Spike: FFI bindings Swift + C#; FFI contract](phase-03-spike-ffi-bindings.md) | A+D | P0 w2 | 01 | Partial (Swift + C# done; ARM64 not executed) |
| 04 | [Spike: macOS clipboard privacy + lazy file paste](phase-04-spike-macos-clipboard-lazy-paste.md) | C | P0 w1-3 | 01 | pending |
| 05 | [Spike: Windows clipboard + virtual-file lazy paste](phase-05-spike-windows-clipboard-lazy-paste.md) | D | P0 w1-3 | 01 | Partial (automated done, ADR 010) |
| 06 | [Core: identity, trust, transport, pairing](phase-06-core-identity-trust-transport.md) | A | P0 w2-3 | 02 | Partial |
| 07 | [Core: clip/1 + file/1 protocols](phase-07-core-protocols-clip-file.md) | A | P0 w3-4 | 06 | Partial |
| 08 | [Core: encrypted history store, FFI surface, CLI harness](phase-08-core-history-ffi-cli.md) | A | P0 w4-6 | 03, 07 | Partial |
| 09 | [Backend: auth, device registry, pairing codes, presence](phase-09-backend-auth-registry-presence.md) | B | P0 w1-4 | 01 | Partial |
| 10 | [Infra: relay + backend deploy, CI](phase-10-infra-relay-backend-deploy.md) | B | P0 w3-5 | 09 | pending |
| 11 | [macOS app: shell, auth, pairing, devices](phase-11-macos-app-shell-pairing.md) | C | P1 w1-5 | 03,04,08,09 | Done on Mac (manual visual checks open) |
| 12 | [macOS app: clipboard, files, history, settings](phase-12-macos-clipboard-files-history.md) | C | P1 w5-11 | 11 | pending |
| 13 | [Windows app: shell, auth, pairing, devices](phase-13-windows-app-shell-pairing.md) | D | P1 w1-5 | 03,05,08,09 | Partial (code + tests + e2e vs CLI pass; Mac pairing open) |
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
4. Developer ID signing: decided and built (team VE5P5RS83W, `build-fsync-mac-app.sh --sign/--profile`); data-protection Keychain migration verified. Notarization verified (`--notary-profile fsync-notary`). Open: signed launch-at-login check (phase 16).
5. Core offline `logout()` leaves the refresh token valid server-side (core change needed).
6. macOS integration script is not in CI (phase 10 to wire it).
7. Optional Low items from phase 11 review round 3 (LoginModel waits for pending clear; boot awaits `sessionClear`).
