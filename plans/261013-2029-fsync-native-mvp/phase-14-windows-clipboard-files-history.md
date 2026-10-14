# Phase 14 — Windows app: clipboard sync, file send, virtual-file paste, history, settings

## Context Links
- Design §4 flows 3-6; ADR 010 ([phase-05](phase-05-spike-windows-clipboard-lazy-paste.md)); [phase-07](phase-07-core-protocols-clip-file.md), [phase-08](phase-08-core-history-ffi-cli.md); ADR 013 (decisions of this phase)

## Overview
- Priority: P1 | Status: done with concerns (merged to main 37e0430, 2026-10-15; Windows 11 x64: code, unit tests and e2e pass; open: manual QA checklist, ARM64 hardware, core findings below) | Effort: 5-6 wk (D) — parallel with phase 12
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
- [x] Virtual-file paste / eager fallback (folders kept both ways; eager only up to 20 MiB, 180 s, reuses earlier copies; inbox retention)
- [x] Manual mode + hotkey (Win+Alt+C send, Win+Alt+V paste pending; tray menu; not Ctrl+Alt = AltGr)
- [x] Send To + drag-drop (shortcut toggle in Settings; drop on a device row; Files page with offers, progress, cancel, retry)
- [x] History + settings (search, paging, pin, delete, clear, copy-back; General / Privacy / Storage)
- [x] Exit flush behavior (virtual files or unrendered formats cleared, rendered entries flushed; quit bounded to ~3 s)
- [x] Tests (unit 117/117; e2e `clipboard-with-cli.ps1` and `pair-with-cli.ps1` PASS)
- [ ] ARM64 hardware run (ARM64 build compiles; no ARM64 host)
- [ ] Manual QA checklist (below)
- Sync 2026-10-15: merged to main (37e0430); Windows dotnet 144/144 on merged main incl. S5 (7cc8cfd). Final review round 4 scored 9.6 (plans/reports/code-reviewer-261015-0200-windows-phase-14-round-4.md); round 3 below was 9.0.

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

## Review round 1 (score 6.5, fixed 2026-10-15)
Merged origin/main (engine.lock, logout revoke, read-ahead); `fsync_ffi.dll` rebuilt (contract still v2, bindings
not regenerated). Changes (ADR 013 §4-§14):
- Privacy: built-in password-manager list ∪ user list always; writer read with the clipboard open, NULL owner →
  foreground app; FsyncOrigin checked first; received clips carry `ExcludeClipboardContentFromMonitorProcessing`,
  `CanIncludeInClipboardHistory=0`, `CanUploadToCloudClipboard=0`. ADR 013 §4 now lists exactly the formats checked.
- Inbox: `InboxDirectory` (hex/dash ids) with macOS retention (days, then size) at start, daily, on settings change;
  copy-back reuses a complete earlier download (no "name (1)").
- Lanes: publish and write one at a time, newest wins, newer clip cancels older downloads; replaced entries release
  their streams; all tasks observed, all failures logged.
- Eager: fallback only up to 20 MiB per clip (else lazy), 180 s timeout, cancellation cancels transfers; core keeps
  `rel_path` layout and CF_HDROP lists top-level entries, so folders paste as folders (verified in e2e).
- Lifecycle: quit bounded (~3 s; engine-call gate refuses new calls, waits for running ones, no network at exit);
  `InitializeAsync` disposes the old engine; engine.lock → "fsync is already running for this data folder", no retry;
  data dir normalized (full path, no trailing slash, long names, case) for the instance key.
- Also: StaThread wake-up via MsgWaitForMultipleObjectsEx + event (no lost wake-up), dispose fails queued work and
  never self-joins; history search debounce (300 ms) with stale-result guard; offers stay until their pulls start;
  auto-accept capped at 1 GiB; hotkeys Win+Alt+C / Win+Alt+V (Ctrl+Alt = AltGr); reserved device names refused;
  images bounded (16384 px, 64 MP), colour table overflow fixed; Send to ignores UNC / `\\?\`; sizes in MiB / GiB;
  the unhandled-exception log has type, HRESULT and stack only.
- Tests: `dotnet test` CoreBindings 9, Interop 47, App 61 (117/117). New: eager path on the real engine (reuse,
  folder layout, flags, echo), eager failure, eager cap, inbox retention, engine-call gate, second engine on one
  folder (engine.lock message), search debounce, offer kept + 1 GiB cap, instance key spellings, private flags vs
  echo marker, denylist union, reserved names, image bounds, StaThread stress and dispose.
- E2E `clipboard-with-cli.ps1` (13 steps) and `pair-with-cli.ps1` PASS; new steps: echo not published back (W1
  app.log), second launch with another spelling of the data folder hands over, lazy folder paste keeps the layout,
  eager fallback downloads a folder with its layout (Win+Alt+V).
- Throughput (real core, two app instances, loopback direct path, 200 MiB lazy paste, done = file closed):
  read-ahead core 1.5-1.6 s (125-130 MiB/s); pre-read-ahead core 85f8968 1.5-1.6 s (126-130 MiB/s), runs
  interleaved, hashes ok. No gain at ~0 RTT, as expected; relay / WAN not measurable on this machine. The spike's
  fake-remote run (no core) is unchanged: 200 MiB in 13.1 s at 2 ms per read.

## Review round 2 (score 9.2, fixed 2026-10-15)
- M1 inbox cleanup keeps every folder a download is writing into (in-flight set, registered before the folder
  exists) and anything younger than 5 min; cleans are serialized. M2 quit tears down the clipboard and the engine
  off the UI thread, then exits. M3 the Done event can no longer be lost between registration and arrival (both
  sides re-check). L1-L9: timers are not touched after dispose (listener released first), copy-back / paste-pending
  report the real write result (pending kept on failure), write cancellation sources are disposed (WriteTicket), an
  eager clip with no usable names fails instead of an empty CF_HDROP, clipboard start is observed and a missing
  engine is caught, Accept ignores a second click while running.
- L10 (not changed, core is owner A): the app detects engine.lock by the text of core's `Invalid` message
  ("already running"); a dedicated error case in core would make this robust.
- Tests: CoreBindings 9, Interop 47, App 67 (123/123, three runs). New, on the real engine: cleanup vs in-flight,
  the Done-event race (both orders and 2000 concurrent), eager timeout cancels its transfer, a newer clip cancels
  the older clip's downloads, publish supersede, pending kept when the write fails. E2E: `clipboard-with-cli.ps1`
  13/13 and `pair-with-cli.ps1` 9/9, twice each. The text-out step re-copies if `rdpclip` (RDP session) re-posts
  the clipboard with `CanIncludeInClipboardHistory=0`, which fsync correctly skips.

## Review round 3 (score 9.0, fixed 2026-10-15)
- H1: one budget (180 s) and one cancellation cover both starting the eager transfers (fetch_file_to can block
  through the dial) and waiting for them; each id is registered as soon as fetch_file_to returns and the finally
  cancels every started, unfinished transfer, including one whose start returns after we stopped waiting.
- M2: quit stops the 15 s account retry first and ignores phase changes while tearing down; GoOnlineAsync treats
  a missing engine (InvalidOperationException) as nothing to do.
- L1/L2: a download's in-flight mark is taken under the cleanup lock and the kept set is read inside it; a reused
  inbox folder is touched, so the 5 min minimum age protects it too.
- Tests (124/124: CoreBindings 9, Interop 47, App 68): the real DownloadAsync start path on the real engine (budget
  spent before the first start, a newer clip stopping an older download before any transfer starts, a failing start
  releasing the folder and touching a reused one). Cancelling a transfer that already started needs a real offer
  from a peer, so it is covered by the e2e eager step (happy path) and by code review, not by a unit test.
  E2E `clipboard-with-cli.ps1` 13/13 and `pair-with-cli.ps1` 9/9, twice each.

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
- **Read-ahead (track C), landed:** Explorer still reads 256 KiB per call, one in flight (ADR 010 §6); with core
  read-ahead a 200 MiB lazy paste runs at ~128 MiB/s on a loopback path, same as without (round 1 above); the gain
  is expected on relay / WAN paths, not measured here. No app change was needed.
  Relay-path throughput not measured here (the app has no force-relay switch).
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
