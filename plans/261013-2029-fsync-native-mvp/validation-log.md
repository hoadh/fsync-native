# Validation Log (fsync-native MVP)

Moved from plan.md on 2026-10-14 to keep the overview under 80 lines. Content unchanged.

## Session 1 — 2026-10-13
**Trigger:** `/ck:plan validate` before implementation
**Questions asked:** 8

### Questions & Answers
1. **[Architecture]** Auth: plan proposes own passwordless email OTP in Rust axum (AuthProvider trait for later swap). Decide?
   - Options: Own email OTP (Recommended) | Managed provider (Clerk/Auth0/Supabase) | OTP + Sign in with Apple/Google in MVP
   - **Answer:** Own email OTP
   - **Rationale:** no vendor cost; trust root is device keys so auth compromise ≠ data leak.
2. **[Scope]** Minimum OS versions?
   - Options: macOS 13+, Win 10 22H2+ & 11 (Recommended) | macOS 14+, Win 11 only | macOS 13+, Win 11 only
   - **Answer:** macOS 13+, Win 10 22H2+ & 11
   - **Rationale:** widest reach; Win10 adds test matrix cost (phase 15).
3. **[Assumptions]** Account recovery when all devices lost?
   - Options: Trust reset + re-pair (Recommended) | Recovery key
   - **Answer:** Trust reset + re-pair
   - **Rationale:** history is local, nothing lost server-side; KISS.
4. **[Risks]** Telemetry / crash reporting?
   - Options: Opt-in crash only (Recommended) | Crash + anonymous usage (opt-out) | Nothing
   - **Answer:** Opt-in crash only
   - **Rationale:** privacy-first product; never send clipboard content/file names.
5. **[Risks]** Encrypt local history?
   - Options: Encrypt in MVP with key in Keychain/DPAPI (Recommended) | Rely on FileVault/BitLocker, defer P2
   - **Answer:** Encrypt in MVP
   - **Rationale:** clipboard may hold secrets; same-user processes could read plain DB. +3-5 d core.
6. **[Tradeoffs]** Relay regions at launch?
   - Options: 2 regions SG + US/EU (Recommended) | 1 region | 3+ regions
   - **Answer:** 2 regions
   - **Rationale:** acceptable relay latency for Asia + West.
7. **[Assumptions]** MVP default limits (inline 256 KiB, clip 50 MiB, eager file 512 MiB, direct transfer unlimited)?
   - Options: OK defaults (Recommended) | Stricter relay caps now
   - **Answer:** OK defaults
   - **Rationale:** configurable; tier caps deferred to P3.
8. **[Architecture]** Windows UI framework?
   - Options: WinUI 3 (Recommended) | WPF
   - **Answer:** WinUI 3
   - **Rationale:** keep plan as-is; tray via H.NotifyIcon.

### Confirmed Decisions
- Auth: own email OTP · Min OS: macOS 13+/Win10 22H2+ · Recovery: trust reset · Telemetry: opt-in crash · Limits: defaults · Win UI: WinUI 3

### Action Items
- [x] History encryption added to phase 08 (SQLCipher + XChaCha blobs, key-loss recovery), `history_key` in FFI contract (03), key storage in 11/13
- [x] 2-region relay in phase 10 (RelayMap, cross-region test)
- [x] ADRs 003/006 confirmed, 011/012 added in phase 01

### Impact on Phases
- Phase 01: ADR table updated (003, 006 confirmed; 011 history at rest; 012 recovery).
- Phase 03: EngineConfig gains `history_key`.
- Phase 08: +3-5 d; encryption requirements, steps, todo, risk (SQLCipher cross-compile), security.
- Phase 10: 2 relay regions, RelayMap config, cross-region risk.
- Phase 11/13: generate + store history key in Keychain/DPAPI.
