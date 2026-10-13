# Brainstorm: fsync-native — cross-platform clipboard & file sync (macOS + Windows)

Date: 2026-10-13 | Status: design approved

## 1. Problem & requirements
- Native app macOS + Windows, commercial product (multi-user, accounts).
- Auto prefer LAN; fallback Internet when not same LAN.
- Detect registered devices + online/offline; sync when online.
- Features: clipboard sync (text, rich text, image, **copied files — in MVP**), fast file transfer, local history + cleanup.
- Team 2-4, target 3-6 months.

## 2. Decisions
| Topic | Decision |
|---|---|
| Stack | Shared Rust core `fsync-core` + native UI: SwiftUI (macOS menubar), WinUI 3 + H.NotifyIcon (Windows tray) |
| Bindings | UniFFI → Swift; uniffi-bindgen-cs → C# (fallback: C ABI + csbindgen) |
| Networking | iroh 1.x (QUIC, E2E via device keys, hole punching, relay fallback); self-hosted iroh-relay |
| Identity | Account (email/OAuth) + device pairing via QR/6-digit code; existing device signs new device into signed device list. Server cannot add devices / read data |
| Clipboard mode | User setting: auto or manual (hotkey/menu) |
| History | Local per device only (SQLite + blob dir), retention by days/size, manual delete |
| Offline devices | No catch-up delivery (no server storage) |

## 3. Approaches evaluated
- Stack: (A) Rust core + native UI ✅ — native UX, logic once; cost: FFI layer. (B) 2 pure-native codebases — logic/protocol/crypto duplicated, drift bugs. (C) Tauri/Flutter — fastest, not truly native, weaker clipboard/tray integration.
- Internet: (A) P2P + self-hosted relay ✅ — low cost, E2E. (B) Require Tailscale — bad UX for product. (C) Cloud upload — bandwidth/storage cost, slower, server sees data.
- Identity: account+pairing ✅ vs account only (password leak = clipboard leak) vs pair only (hard to monetize/manage).
- History: local ✅ vs synced history (conflict/deletion propagation complexity) vs server-stored (cost, liability).

## 4. Architecture
```
macOS app (SwiftUI)        Windows app (WinUI 3)
   clipboard watcher          clipboard watcher
        \  UniFFI                /  UniFFI-cs
         fsync-core (Rust)
          - iroh endpoint: identity, LAN discovery, hole punch, relay
          - protocols: clip/1, file/1 (blake3 verified, resumable)
          - trust: signed device list, accept only paired NodeIds
          - history: SQLite + blobs, retention
        |                         |
   Backend (auth, device registry, presence; Postgres)   iroh-relay (self-hosted)
```

### Flows
1. Pairing: new device login → gen keypair → QR/code → existing device verifies + signs updated device list → upload.
2. Presence: WebSocket heartbeat to backend + iroh direct connection state. UI states: online-LAN / online-relay / offline.
3. Clipboard: change detect → hash → skip self-written (echo loop) → sensitive filter → size limit → send to online peers.
4. Files: drag-drop / "Send to…"; streamed, blake3 chunk verification, resume.
5. Copy-file-paste (lazy): sender announces metadata only; receiver exposes lazy clipboard entry; bytes fetched on paste.
   - macOS: `NSFilePromiseProvider` / file promises.
   - Windows: virtual files `CFSTR_FILEDESCRIPTOR` + `CFSTR_FILECONTENTS` via custom `IDataObject` (delayed rendering).
6. History: search, pin, delete item/all, auto cleanup.

## 5. Risks & mitigations
| Risk | Level | Mitigation |
|---|---|---|
| Lazy copy-file-paste (2 very different OS APIs, app-specific quirks; Explorer virtual files, Finder promises) | High | Spike in P0 on both OS before committing; fallback = eager download to temp dir then put real file paths on clipboard (with size cap) |
| macOS clipboard monitoring: polling `changeCount`; possible privacy prompts on newer macOS | Med | Verify on current macOS early; efficient polling interval |
| Sensitive data leak | Med | Honor `org.nspasteboard.ConcealedType`/`TransientType` (mac), `ExcludeClipboardContentFromMonitorProcessing`/`CanIncludeInClipboardHistory` (win); app denylist |
| C# bindings maturity | Med | Evaluate uniffi-bindgen-cs in P0; fallback C ABI |
| WinUI 3 tray support | Low-Med | H.NotifyIcon; WPF as fallback |
| iroh mDNS/local discovery status in 1.x unconfirmed | Med | Verify in docs during P0; fallback own mDNS + direct addrs |
| Relay bandwidth cost | Low | Rate/bandwidth caps per plan tier |
| Distribution | Med | Developer ID + notarization (outside App Store), Azure Trusted Signing; auto-update Sparkle / Velopack |

## 6. Phases (copy-file-paste kept in MVP → MVP +3-4 wk)
- **P0 (~4-5 wk)**: fsync-core (iroh, LAN+relay, clip/file protocols, SQLite) + CLI harness; backend (auth, device registry, presence); bindings spike; **lazy file paste spike on both OS**.
- **P1 MVP (~11-14 wk)**: Mac + Win apps — tray/menubar, QR pairing, device list + status, clipboard text/rich/image/files (auto/manual, filters), file send, history + cleanup, signing + auto-update.
- **P2**: per-device targeting, folders/very large files, polish.
- **P3**: billing, plan limits on relay, teams/orgs, other platforms.
- Team split: Rust core / backend+DevOps / macOS / Windows.

## 7. Success metrics
- LAN file throughput ≥ 80% of link speed; clipboard latency LAN < 300 ms, relay < 1.5 s.
- Direct-connection ratio measured (target majority of bytes direct).
- Zero sensitive (concealed) items synced in tests.
- Copy-file-paste works in Finder ↔ Explorer + top target apps.

## 8. Next steps
- Create implementation plan (`/ck:plan`) from this report.
- P0 spikes: iroh local discovery, uniffi-bindgen-cs, lazy file paste both OS, macOS clipboard privacy behavior.

## Unresolved questions
- Auth provider choice (own vs Clerk/Auth0/Supabase) & backend language.
- Max clipboard/file size limits per plan; pricing model.
- Minimum OS versions (macOS ≥ 13? Windows 10 or 11 only?).
- Account recovery when all devices lost (re-pair from scratch acceptable?).
- Telemetry/crash reporting policy (privacy).

Sources: https://docs.rs/crate/iroh/1.0.3, https://docs.rs/crate/iroh-relay/1.0.0, https://pinggy.io/blog/iroh_1_0_dial_keys_not_ips/
