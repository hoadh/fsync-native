# Phase 14 — Windows app: clipboard sync, file send, virtual-file paste, history, settings

## Context Links
- Design §4 flows 3-6; ADR 010 ([phase-05](phase-05-spike-windows-clipboard-lazy-paste.md)); [phase-07](phase-07-core-protocols-clip-file.md), [phase-08](phase-08-core-history-ffi-cli.md); ADR 013 (decisions of this phase)

## Overview
- Priority: P1 | Status: done with concerns (2026-10-14; Windows 11 x64: code, unit tests and e2e pass; open items below) | Effort: 5-6 wk (D) — parallel with phase 12
- Productionize spike: listener + filters, auto/manual, format mapping, delayed-render writer, virtual-file paste (or eager per ADR 010), drag-drop + Explorer "Send to", history, settings.

## Key Insights
- Format map: `CF_UNICODETEXT`↔`text/plain`, `HTML Format`(CF_HTML header wrap/unwrap)↔`text/html`, `Rich Text Format`↔`text/rtf`, `PNG`/`CF_DIBV5`→`image/png`, `CF_HDROP`→files; incoming files → `FILEDESCRIPTORW`/`FILECONTENTS` (lazy) or `CF_HDROP` (eager).
- Large non-file reps (big image) use delayed rendering (`WM_RENDERFORMAT` / IDataObject GetData → `fetch_rep`).
- Explorer "Send to" = shortcut in `shell:sendto` invoking `Fsync.exe --send <paths>` → IPC to running instance (AppInstance redirection).

## Requirements
- Same functional set as phase 12 (parity table in phase 15).
- Sensitive: skip on `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, `CanUploadToCloudClipboard=0`; process denylist via `GetClipboardOwner`→PID→exe name.
- Exit with lazy entry on clipboard: render small reps (`OleFlushClipboard` w/ eager small files ≤ 20 MiB) else clear our entry.

## Architecture (as built)
```
apps/windows/src/Fsync.Interop/          Win32 / COM, no app logic
  Clipboard/ ClipboardListener (+hotkeys), StaThread, OleClipboard, Win32Clipboard, ClipboardReader,
             SensitiveFormatFilter, ClipboardOwner, ClipboardFormats, CfHtmlCodec, DropFiles,
             ClipEntry (eager | delayed), ClipDataObject(.Feedback) (IDataObject + async capability)
  Shell/     VirtualFile, RemoteFileStream (IStream → read_file_range), ShellFormats, ShellLink (+KnownFolders)
  Imaging/   ImageCodec (CF_DIB ↔ PNG via WIC)
apps/windows/src/Fsync.App.Core/
  Clipboard/ ClipboardService(.Incoming), RepresentationMapper, IncomingClip (+FileClipStrategy)
  Files/     SendTo (shortcut + --send args), TransfersViewModel, TransferItem (+IncomingOffer)
  ViewModels/HistoryViewModel, ClipSettingsViewModel;  Services/ AppPreferences, AppLog
  Bridge/    EngineBridge.Clips (RemoteClip / IncomingFiles / Transfer events, blocking Call)
apps/windows/src/Fsync.App/  Views/{HistoryPage, TransfersPage, SettingsPage (General/Privacy/Storage)},
  DevicesPage (drop target), TrayHost (clipboard menu), Program (--send redirect)
tests/Fsync.Interop.Tests (+ Fsync.Interop.PasteHelper), tests/Fsync.App.Tests, tests/e2e/{E2eHost,ClipTools}.psm1,
  tests/e2e/clipboard-with-cli.ps1
```
Data flow out: WM_CLIPBOARDUPDATE (150 ms settle) → skip rules + denylist → mapper → `publish_clip`. In: `on_remote_clip` → (auto) `ClipDataObject`: reps ≤ 256 KiB fetched first, larger delayed (`fetch_rep` on first read); files as virtual files (`read_file_range` per Explorer read) or, eager strategy, `fetch_file_to` into the inbox then CF_HDROP.

## Implementation Steps
1. Listener + filter + mapper (CF_HTML codec unit tests).
2. Writer with origin format + delayed rendering for large reps.
3. Virtual files per ADR 010 incl. async capability + progress; eager fallback path + inbox cleanup.
4. Manual mode + global hotkey (`RegisterHotKey`).
5. Send To shortcut + drag-drop to flyout.
6. History window + settings.
7. Exit/flush behavior; ARM64 verification.
8. xUnit tests for mapper/filter/CF_HTML; manual QA checklist.

## Todo List
- [x] Listener/filter/mapper (skip rules before any `GetClipboardData`; app denylist by owner exe)
- [x] Writer + delayed rendering (`ClipEntry.Lazy`, FsyncOrigin marker, `PNG` + `CF_DIB` for images)
- [x] Virtual-file paste / eager fallback (folders included; eager = "Download copied files before pasting")
- [x] Manual mode + hotkey (Ctrl+Alt+C send, Ctrl+Alt+V paste pending; tray menu)
- [x] Send To + drag-drop (shortcut toggle in Settings; drop on a device row; Files page with offers, progress, cancel, retry)
- [x] History + settings (search, paging, pin, delete, clear, copy-back; General / Privacy / Storage)
- [x] Exit flush behavior (virtual files cleared; lazy reps ≤ 20 MiB rendered + flushed; larger cleared)
- [x] Tests (unit 92/92; e2e `clipboard-with-cli.ps1` and `pair-with-cli.ps1` PASS)
- [ ] ARM64 hardware run (ARM64 build compiles; no ARM64 host)
- [ ] Manual QA checklist (below)

## Verification (Windows 11 x64, 2026-10-14)
- `dotnet test apps/windows/Fsync.sln`: CoreBindings 9, Interop 31, App 52 — all pass. Interop tests use the real
  clipboard and the shell's copy engine (Paste verb from `Fsync.Interop.PasteHelper`): files + folders + empty file
  pasted with matching hashes; a failing remote read ends the paste without a complete file. App tests round-trip
  text / HTML / RTF / images / files through the real clipboard and run settings, history and transfers on the real
  engine (offline). No mocks in phase-14 tests (phase-13 tests keep their fake engine).
- `apps/windows/tests/e2e/clipboard-with-cli.ps1` PASS (two app instances W1, W2 + fsync-cli devA, local relay +
  backend): text W1 → devA 0.3 s; HTML → devA as rich; a copy marked `ExcludeClipboardContentFromMonitorProcessing`
  is not sent; devA text → Windows clipboard 0.6-0.8 s; history lists it and Copy puts it back; devA pushes a 3 MiB
  file, accepted in the Files page, saved intact; Explorer "Send to" (second launch `--send`) pushes 2 MiB to devA
  intact; W2 in manual mode gets a file copied on W1 with Ctrl+Alt+V and the shell pastes 5 MiB streamed from W1 in
  0.9 s, hash ok. `pair-with-cli.ps1` (phase 13) still passes on the shared harness. Final loop: both scripts
  twice each, all PASS.
- Fixed while testing: a second click on a button that opens a confirmation (ContentDialog) crashed the app
  ("Only a single ContentDialog can be open"); all dialogs now go through `Views/Dialogs`. Unhandled exceptions are
  logged to `app.log`.
- ARM64: `dotnet build src/Fsync.App -c Release -p:Platform=ARM64 -r win-arm64` succeeds (not executed).

## Deviations
- Clipboard owner is a dedicated STA thread, not the UI thread (ADR 013 §1): delayed renders block on `fetch_rep`.
- One Settings page with General / Privacy / Storage sections instead of three pages; Transfers live on a "Files" page
  (also the Send-to device picker); the "flyout" drop target is the device list in the main window.
- History Copy works only for clips received while the app runs: the FFI has no call to read a stored item's
  content (owner A: add e.g. `history_item(id)` / `fetch_history_rep`). Own copies and older items show Copy disabled.
- Pull retry: a pull that fails with a network error is retried once after 20 s, and failed pulls get a Retry button
  (mitigation for the core issue below).
- E2E harness: `tests/e2e/E2eHost.psm1` (shared with `pair-with-cli.ps1`), local iroh relay always started (as in
  `p0-exit-demo.sh`), Postgres from `FSYNC_E2E_PG_BIN` when docker is unavailable; `FSYNC_DOWNLOAD_DIR` env.
- Main window widened to 720 px so the five top-level tabs do not overflow into "More".

## Findings for other tracks
- **Core (owner A), dial to a restarted peer:** after this device has talked to peer P, and P's process restarts
  (same node id, new port), dials to P time out (`network: dial timed out`, 10 s) for ~30 s; a pull started 30-60 s
  later works. Reproduced with the app: W1 publishes to `devA clip watch`, the watch process exits, `devA send` (new
  process) pushes to W1, W1's `fetch_file_to` dial times out (100% in 4 runs). Not seen CLI ↔ CLI when the receiver
  never dialed the old process. Likely stale iroh path state; `read_file_range` (lazy paste) would hit the same window
  after a sender restart. App mitigates with one retry (above).
- **Read-ahead (track C):** lazy paste reads 256 KiB per Explorer call, one in flight (ADR 010 §6); local paste of
  5 MiB took 0.9 s. Over a relay, throughput = 256 KiB / RTT until `read_file_range` reads ahead. No app change needed.
- `fetch_file_to` returns only after the dial: its Failed event can arrive before the call returns (handled).

## Manual QA checklist (open)
- Real Ctrl+V in an Explorer window / Desktop (automated: shell Paste verb), cancel from the Explorer progress dialog.
- App matrix: Word / Outlook (HTML + RTF), Chrome upload of a virtual file, Teams / Slack, Paint (CF_DIB), VS Code.
- Real password managers (1Password, KeePass, Bitwarden): no item synced; denylist by exe name.
- Hotkeys clash with another app (message shown), drag-drop of folders onto a device, Send to with many files.
- Exit with a received image on the clipboard (survives), with virtual files (cleared).
- Windows 10 22H2; ARM64 hardware; Mac ↔ Windows round trips (track X).

## Success Criteria
- Text/html/image round-trip win↔mac; Explorer paste of 1 GiB remote file with progress + cancel, hash verified; 0 password-manager items synced; idle CPU ~0%.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Clipboard locked by other app (OpenClipboard fails) | H x L | retry w/ backoff (5x 50 ms) |
| COM threading bugs (STA) | M x H | dedicated STA thread for clipboard ops; stress tests |
| Explorer hang on slow relay fetch | M x H | async capability + prefetch; eager for small |

## Rollback
- Strategy enum switch to eager via setting; user can switch to manual mode.

## Security Considerations
- Never store filtered items; inbox `%LOCALAPPDATA%` user ACL; Send To IPC only accepts same-user instance; validate paths.
- `app.log` (data dir) holds names, sizes, counts and states only: no clipboard content, no tokens.

## Next Steps
- Phase 15, 16. Owner A: history content API, dial-after-restart fix, read-ahead. Track X: Mac ↔ Windows clipboard.
