# ADR 013 — Windows app clipboard: owner thread, format map, exit and file strategies
Status: accepted (phase 14, Windows 11 x64, 2026-10-14). Builds on ADR 010 (lazy paste spike).

## Context
Phase 14 turns the phase-05 spike into the app: publish clipboard changes, write received clips,
paste remote files lazily, push files ("Send to", drag-drop), history and settings. The spike
left open where the clipboard owner lives, how formats map to the wire, what happens on exit
for entries that are not virtual files, and how a user falls back when lazy paste does not suit a
target app.

## Decisions
1. **A dedicated STA thread owns the listener and our clipboard entries** (`StaThread`
   "fsync-clipboard"), not the WinUI UI thread (deviation from ADR 010 decision 2). Delayed
   renders call `fetch_rep` synchronously when another app reads a format; on the UI thread that
   would freeze the window for a network round trip. The thread pumps messages, so OLE and the
   listener work as on the UI thread; results reach view models through the dispatcher.
2. **Format map** (`RepresentationMapper`). Out, in rep order (core derives the clip kind from
   the first rep): `HTML Format` (CF_HTML unwrapped to the fragment) → `text/html`,
   `Rich Text Format` → `text/rtf`, `CF_UNICODETEXT` → `text/plain`, `PNG` or `CF_DIBV5`/`CF_DIB`
   (converted with WIC) → `image/png`; `CF_HDROP` → files. In: the reverse; `image/png` is written
   both as `PNG` and as `CF_DIB`. Unknown MIME types are not written.
3. **Eager vs delayed reps.** Reps up to 256 KiB (core's inline budget: already in the offer) are
   fetched before writing; bigger ones are delayed-rendered (`ClipEntry.Lazy`), fetched once on
   first read and shared by the formats of that rep (`PNG` + `CF_DIB`).
4. **Skip before size.** The reader classifies on format names and DWORD flags first and only
   then calls `GetClipboardData` for the formats it may read: `GetClipboardData` forces other
   apps' delayed renders, which a skipped (sensitive) change must not trigger.
5. **Exit policy** (extends ADR 010 decision 4): if our entry is still on the clipboard, an
   entry with virtual files is cleared (never flushed); otherwise lazy formats up to 20 MiB
   still unrendered are fetched and the entry is flushed (`OleFlushClipboard`), so a received text
   or image survives the app; above 20 MiB it is cleared.
6. **File strategy setting**: `Lazy` (default; virtual files, `read_file_range` per Explorer
   read) or `Eager` ("Download copied files before pasting": `fetch_file_to` into
   `%LOCALAPPDATA%\fsync\inbox\<item>`, then `CF_HDROP`), the rollback switch from the plan.
7. **Denylist enforcement** in the app (core only stores it): the clipboard owner window →
   process id → exe name, compared case-insensitively without ".exe". Default list: common
   password managers. Their documented private formats are skipped regardless.
8. **Manual mode** = core `clip_mode = Manual`: nothing is published until Ctrl+Alt+C (or the
   tray), received clips wait as "pending" until Ctrl+Alt+V. Hotkeys are `RegisterHotKey` on the
   listener window (MOD_NOREPEAT); a clash is reported, not fatal.
9. **Send to**: a shortcut in `shell:sendto` runs `Fsync.exe --send <paths>`; a running
   instance receives the command line through `AppInstance` redirection (same user, same data
   dir) and shows a device picker. Only existing absolute paths are kept.
10. **History copy-back** works for clips received while the app runs (core keeps their offers
    in memory). The FFI has no call to read a stored item's content, so older items and this PC's
    own copies show Copy disabled. Needs a core API (owner A) for full parity.

## Consequences
- Throughput of a lazy paste is one 256 KiB chunk per round trip until core reads ahead
  (`read_file_range`, track C). Nothing in the app changes when that lands.
- Clipboard tests use the real clipboard; test assemblies serialize on a machine-wide semaphore.
- E2E (`apps/windows/tests/e2e/clipboard-with-cli.ps1`) needs an interactive desktop session
  (clipboard, UI Automation, hotkeys); over SSH it runs through a scheduled task with `/IT`.
