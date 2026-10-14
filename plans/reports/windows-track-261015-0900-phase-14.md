# Windows track B - phase 14 report

Branch: windows/phase-14 (7 commits, d083b26..6b87203), pushed, tree clean on waso.
Verified by orchestrator over SSH: git status clean and in sync with origin; no diffs vs 85f8968 in core/backend/apps/macos/.github/deploy/shared docs; `dotnet test Fsync.sln` = 92/92 (CoreBindings 9, Interop 31, App 52).

## Delivered
- Clipboard sync (text/HTML/RTF/image, privacy-flag + blocklist skip), manual mode hotkeys, lazy virtual-file paste (ADR 010) with download-first fallback, Send To / drag-drop, Files page (offers, progress, cancel, retry), history (search/pin/delete/clear/re-copy), settings (General/Privacy/Storage), ADR 013.
- E2E (real app x2 + fsync-cli): clipboard, file push both directions, Send To, 5 MiB lazy paste 0.9 s hash-match; phase-13 pair e2e still passes.
- Fixed: app crash on double-opened ContentDialog.

## Open items
- Core (owner A): dial to a restarted peer (same node id) times out ~30 s (app retries once after 20 s + Retry button); need API to read history item content (only current-session received clips re-copyable); read_file_range read-ahead (track C), no app change needed.
- Not run: ARM64 hardware, Windows 10, Mac<->Windows (track X), manual checklist (real Explorer Ctrl+V, cancel mid-copy, Word/Outlook/Chrome, real password manager).
- Env: Docker Desktop on waso broken (containerd panic); portable PG17 at C:\Users\dev\tools\pg17 via FSYNC_E2E_PG_BIN. E2E needs interactive desktop (scheduled task /IT via SSH).
