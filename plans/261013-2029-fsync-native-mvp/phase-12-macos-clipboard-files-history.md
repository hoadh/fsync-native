# Phase 12 — macOS app: clipboard sync, file send, lazy paste, history, settings

## Context Links
- Design §4 flows 3-6; ADR 009 ([phase-04](phase-04-spike-macos-clipboard-lazy-paste.md)); [phase-07](phase-07-core-protocols-clip-file.md), [phase-08](phase-08-core-history-ffi-cli.md)

## Overview
- Priority: P1 | Status: Partial (implemented, automated gates green on merged main, review 9.5; manual QA open: Finder/app paste matrix, real password managers, GUI privacy alert, Mac↔Win app round trip; relay list + pending revoke live in phase 11) | Effort: 5-6 wk (C) — parallel with phase 14
- Productionize spike: watcher + filters, auto/manual modes, reps mapping (text/rtf/html/png/tiff→png/file URLs), write incoming clips, lazy/eager file paste per ADR 009, drag-drop + Share/"Send to…", history UI, retention settings.

## Key Insights
- Representation map (NSPasteboard ↔ MIME): `public.utf8-plain-text`↔`text/plain`, `public.rtf`↔`text/rtf`, `public.html`↔`text/html`, `public.png`/`public.tiff`→`image/png` (convert TIFF), `public.file-url`→files.
- Echo guard: written items include `com.fsync.origin` type; watcher skips them + core hash guard.
- Manual mode: global hotkey (e.g. ⌃⌥C send current clipboard) via `KeyboardShortcuts`-style Carbon hotkey; menu "Send clipboard".

## Requirements
- Auto mode: change → publish within 1 poll interval. Manual mode: only on hotkey/menu.
- Sensitive: concealed/transient/auto-generated types skipped + bundle-id denylist (default: 1Password, Bitwarden, Keychain Access) + user list.
- Incoming in auto mode: write to pasteboard; notify (optional) ; in manual mode: show in menu "Paste from <device>".
- Files: lazy provider calls `fetch_file_to(dest)`; progress in menu; failures → user notification.
- History window: search, pin, delete, clear, copy-back to clipboard.

## Architecture
```
apps/macos/FsyncMac/
  Clipboard/PasteboardWatcher.swift      # adaptive poll (150 ms after change → 1 s idle)
  Clipboard/SensitiveFilter.swift        # types + frontmost bundle id denylist
  Clipboard/RepresentationMapper.swift   # pasteboard ⇄ [Rep]
  Clipboard/ClipboardWriter.swift        # writes incoming, adds origin marker
  Clipboard/LazyFilePromiseWriter.swift  # ADR 009 lazy path
  Clipboard/EagerFileWriter.swift        # fallback path (inbox dir, cap)
  Clipboard/FileClipStrategy.swift       # enum lazy|hybrid|eager + thresholds
  Clipboard/HotkeyManager.swift
  Files/DropTargetView.swift, Files/ShareMenu.swift, Files/TransferListView.swift
  Features/History/{HistoryWindow, HistoryRowView, HistorySearchModel}.swift
  Settings/{GeneralSettingsView, PrivacySettingsView, StorageSettingsView}.swift
```
Data flow out: changeCount → read types → SensitiveFilter → RepresentationMapper → `publish_clip`. In: `on_remote_clip` → (auto) ClipboardWriter (inline reps immediate; large reps via data provider → `fetch_rep`; files via strategy).

## Related Code Files
- Create: above. Modify: `Bridge/EngineBridge.swift`, `Features/Menu/MenuBarContentView.swift`, `Settings/SettingsWindow.swift` (own phase 11 files, same owner).

## Implementation Steps
1. Watcher + filter + mapper; publish; unit tests for mapper/filter.
2. Writer w/ origin marker + lazy data provider for large reps.
3. Files: strategy per ADR 009, inbox cleanup on retention.
4. Manual mode + hotkey + menu actions.
5. Drag-drop onto menubar popover / device row; Finder Share extension optional (YAGNI → "Send to…" via menu + drop first).
6. History window + settings (mode, retention days/size, max clip size, denylist).
7. Onboarding step for pasteboard privacy permission (from ADR 009).
8. Tests: XCTest mapper/filter/echo; manual QA checklist from phase 15.

## Todo List
- [x] Watcher/filter/mapper (adaptive poll, SensitiveFilter, RepresentationMapper; unit tests)
- [x] Writer + echo marker (`com.fsync.origin`, changeCount guard; real-engine tests incl. no-echo integration test)
- [x] File strategy (lazy/hybrid/eager per ADR 009; `FileClipStrategy`, inbox 0700 + cleanup by retention; unit tests with closure seam for `fetch_file_to`; Finder paste of lazy/eager entries is a manual check)
- [x] Manual mode + hotkey (Carbon, Ctrl-Opt-C, menu "Send clipboard", "Paste from")
- [x] Drag-drop / send to ("Send files…" panel + drop on device row in popover; integration tests CLI<->Mac both ways; accept/decline, auto-accept setting, transfer progress + cancel)
- [x] History window (search, pin, delete, clear, load more; integration test)
- [x] Settings (Sync mode + auto-accept files, Privacy denylist, Storage retention/size/max clip/file strategy)
- [x] Privacy onboarding (explanation + state from `NSPasteboard.accessBehavior`; the real macOS alert is unverified, see ADR 009 open items)
- [x] Tests (FsyncMac 188 tests/12 integration skipped without backend, 0 failures; integration script 12/12 PASS on merged main; FsyncCoreKit 8)
- [ ] Manual QA checklist (phase 15): Finder/Mail/Messages/Slack/VS Code/Chrome paste, 1Password/Keychain, privacy alert, mac<->win round trip
- [ ] Review medium: sync stays off after a failed initial settings load (add retry or re-read path)
- [ ] History copy-back of full content/images/files (blocked on FFI, see Deviations)

## Deviations
- Files live under `FsyncMac/Sources/FsyncMacCore/{Clipboard,Files,Features/History,Settings}` and `FsyncMacUI/{Menu,History,Settings,Devices}`, not the phase's Architecture tree; lazy/eager writers are one `FileClipWriter` (+ `FileClipStrategy`, `InboxDirectory`, `FileFetchService`, `TransfersModel`, `FilesSync`).
- Hotkey is Carbon Ctrl-Opt-C, fixed (no customization UI).
- History copy-back restores only the 200-char text preview: core history stores previews, no content accessor (FFI gap, see report).
- Sensitive-filtered clips are skipped entirely (never sent, never in history).
- Folders are out of the MVP (files arrive flat); drop accepts plain files only.
- `FileClipStrategy` is stored in UserDefaults (key `fileClipStrategy`), editable in Settings > Storage.
- Share extension not done (YAGNI per step 5).
- Inbox cleanup runs when the engine becomes ready (retention days, current clip kept), not on a timer.
- `run-fsync-mac-integration.sh` runs test classes in two groups with a backend restart in between (dev backend allows 20 OTP codes/hour/IP) and filters on the new classes.

- Round 2 (review): settings are read off the main actor and clipboard sync starts only after a successful
  read (writes are refused until then). History copy-back is offered only for text stored in full (the core
  keeps a 200-char preview). Auto-accept capped at 1 GiB per offer, pending offers capped at 20, cancel aborts
  the rest of an offer, paste timeout 180 s. Inbox cleanup runs at start, on retention change and daily, and
  applies the byte cap. Publishing is serialized with the newest copy queued, and the pasteboard changeCount
  is re-checked before read and before send.

## Success Criteria
- Text/rich/image round-trip mac↔mac & mac↔win; Finder paste of remote file (lazy or eager per ADR); 0 concealed items synced in filter test suite; idle CPU < 0.5%.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Rich text fidelity across OS (RTF vs HTML) | H x M | always send text/plain + html; RTF mac↔mac only |
| Large images (screenshots TIFF) | M x M | convert to PNG, size cap, lazy rep fetch |
| Polling misses fast double copies | L x L | acceptable; last-wins |

## Rollback
- `FileClipStrategy` switchable remotely? No (YAGNI) — app setting + release. Auto mode can be disabled by user (manual) as safety valve.

## Security Considerations
- Never persist sensitive-filtered items; denylist editable; history window behind no extra auth (note in unresolved Qs); inbox dir user-only perms.

## Next Steps
- Phase 15 cross-OS tests; phase 16 packaging.
