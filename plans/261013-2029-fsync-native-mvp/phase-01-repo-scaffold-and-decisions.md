# Phase 01 — Repo scaffold + P0 decisions

## Context Links
- Design: [brainstorm](../reports/brainstorm-261013-2029-cross-platform-clipboard-file-sync-design.md) §2, §8, unresolved Qs
- Plan: [plan.md](plan.md)

## Overview
- Priority: P0 (blocks everything) | Status: Partial | Effort: 2-3 days (B lead, all review)
- Create monorepo, CI skeleton, decision records (ADRs) for open items; start long-lead procurement (certs, accounts).

## Key Insights
- Backend language/auth unresolved in design. Recommendation below; record as ADR, do not block other phases on auth vendor (backend auth module behind trait).
- Signing identities have lead time (Apple Developer org enrollment needs D-U-N-S; Azure Trusted Signing identity validation days-weeks) → start now, used in phase 16.
- One Cargo workspace at root lets backend + core share `fsync-proto` (device list, API DTOs) → no drift.

## Requirements
- Functional: repo builds empty crates/apps on CI (macOS + Windows + Linux runners).
- Non-functional: every code file < 200 lines; kebab-case for docs/scripts; language conventions for code (snake_case.rs, PascalCase.swift/.cs).

## Architecture — monorepo layout
```
fsync-native/
  Cargo.toml                 # workspace: core/*, backend
  rust-toolchain.toml
  core/
    fsync-proto/             # wire types, DeviceList, API DTOs, postcard codecs (shared w/ backend)
    fsync-core/              # engine: endpoint, trust, pairing, clip, file, history, retention
    fsync-ffi/               # UniFFI exports only (thin adapters over fsync-core)
    fsync-cli/               # test harness binary (pair, send, watch, bench)
  backend/                   # axum service + migrations/
  apps/
    macos/                   # Xcode project FsyncMac + Packages/FsyncCoreKit (generated Swift + xcframework)
    windows/                 # Fsync.sln: Fsync.App (WinUI3), Fsync.Interop (Win32/COM), Fsync.CoreBindings (generated C#)
  deploy/
    relay/                   # iroh-relay config, compose/terraform
    backend/                 # Dockerfile, compose, env templates (no secrets)
  scripts/                   # build-xcframework.sh, build-windows-dll.ps1, gen-bindings.sh
  tests/e2e/                 # cross-OS scenarios, netem/NAT topology scripts
  spikes/                    # throwaway P0 code; deleted/archived at end of P0
  docs/                      # system-architecture.md, code-standards.md, protocol-spec.md, adr/
  .github/workflows/
```

### Decisions (ADRs in docs/adr/)
| ADR | Recommendation | Rationale |
|---|---|---|
| 001 Backend lang | Rust + axum + sqlx + Postgres | Shares `fsync-proto` (DeviceList verify server-side, DTOs); one toolchain for A/B; low mem footprint |
| 002 Auth | Own passwordless email OTP + JWT (access 15 m, refresh 30 d rotating); OIDC (Apple/Google/MS) in P2 | Trust root is device keys, not account → auth compromise ≠ data leak; no per-MAU vendor cost; ~300 LoC. `AuthProvider` trait so swap to managed OIDC (verify JWKS) costs < 1 wk |
| 003 Min OS | macOS 13+ (MenuBarExtra), Windows 10 22H2+/11, x64+ARM64 — **confirmed (validation S1)** | WinAppSDK supports 1809+; Win10 kept for reach, budget test cost |
| 004 Serialization | postcard (serde) for wire + DeviceList canonical bytes; JSON for HTTP | compact, deterministic for signatures |
| 005 Discovery | Custom iroh `Discovery` fed by backend presence + iroh local-network discovery (pending phase 02) | no dependency on n0 public DNS/pkarr |
| 006 Crash/telemetry | Opt-in crash reports only, no content — **confirmed** | privacy |
| 011 History at rest | SQLCipher + XChaCha blobs, key in Keychain/DPAPI — **MVP (validation S1)** | clipboard may hold secrets; other same-user processes can read plain DB |
| 012 Account recovery | Lost all devices → re-genesis device list, old devices warn "trust reset" — **confirmed** | no recovery key in MVP (KISS) |

## Related Code Files
- Create: `Cargo.toml`, `rust-toolchain.toml`, `core/*/Cargo.toml` + `src/lib.rs` stubs, `backend/Cargo.toml`, `.github/workflows/ci.yml`, `docs/adr/001..006-*.md`, `docs/adr/011-history-encryption-at-rest.md`, `docs/adr/012-account-recovery-trust-reset.md`, `docs/code-standards.md`, `.gitignore`, `.editorconfig`
- Modify: none | Delete: none

## Implementation Steps
1. `git init`; root workspace; empty crates; `cargo check` green.
2. CI: rust fmt/clippy/test on ubuntu+macos+windows; placeholder jobs for apps.
3. Write ADRs 001-006; lead + team sign-off (30-min review).
4. Write `docs/code-standards.md` (200-line rule, module split rules, error handling: `thiserror` in libs, `anyhow` only in bins).
5. Procurement: Apple Developer Program (org), Developer ID cert; Azure Trusted Signing account + identity validation; domain + DNS for `api.` and `relay.` hosts; transactional email provider.
6. Create `spikes/` with one subfolder per spike owner.

## Todo List
- [ ] Repo + workspace + CI green
- [ ] ADRs 001-006 signed off
- [x] code-standards.md
- [ ] Apple + Azure signing enrollment started (track ticket)
- [ ] Domains/DNS + email provider account

## Implementation Notes (sync 2026-10-14)
- Done: Cargo workspace (`core/*`, `backend`), `.github/workflows/ci.yml` (ubuntu/macos/windows matrix + macOS and Windows app jobs), `docs/code-standards.md`, ADR 001-006, 011, 012. ADR 007 (iroh) and 008 (FFI) added during P0.
- Deviation: step 1 `git init` not done. Repo has no `.git`, so "CI green" and "ADRs merged" are unverifiable here.
- Not verified: CI run status on 3 OS.
- Open, user-owned: ADR 001-006 sign-off (lead + team review); Apple Developer and Azure Trusted Signing enrollment; domains/DNS; transactional email provider (plan Q3).
- Housekeeping: `.gitignore` has only `/target`, so `spikes/iroh-discovery/target/` is not ignored.

## Success Criteria
- CI green on 3 OS for empty workspace; ADRs merged; procurement tickets opened with owners/dates.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Signing enrollment delays | M x H | start day 1; phase 16 can use ad-hoc/self-signed for internal beta |
| ADR bikeshedding | M x M | timebox 1 day; lead decides |

## Rollback
- Pure scaffolding; revert commits. ADR changes = new superseding ADR.

## Security Considerations
- No secrets in repo; `.env.example` only; CI secrets via GitHub environments.

## Next Steps
- Unblocks 02, 03, 04, 05, 09.
