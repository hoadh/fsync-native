# ADR 013 — Windows app clipboard: owner thread, format map, exit and file strategies
Status: accepted (phase 14, Windows 11 x64, 2026-10-14; revised after review round 1, 2026-10-15). Builds on
ADR 010 (lazy paste spike).

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
   apps' delayed renders, which a skipped (sensitive) change must not trigger. Checked, in this
   order: `FsyncOrigin` (our own write), `ExcludeClipboardContentFromMonitorProcessing`,
   `Clipboard Viewer Ignore`, `CanIncludeInClipboardHistory` = 0, `CanUploadToCloudClipboard` = 0.
   No other "private" formats are checked; password managers that write none of these are caught
   only by the denylist (decision 7).
5. **Exit policy** (extends ADR 010 decision 4): if our entry is still on the clipboard and it has
   virtual files or a format nobody rendered yet, it is cleared (never flushed, no network at
   exit); a fully rendered entry is flushed (`OleFlushClipboard`), so a received text or image
   survives the app. Quit takes at most ~3 s for clipboard work (decision 14).
6. **File strategy setting**: `Lazy` (default; virtual files, `read_file_range` per Explorer read)
   or `Eager` ("Download copied files before pasting"), a compatibility fallback that applies only
   to clips up to 20 MiB in total (larger ones stay lazy): `fetch_file_to` into
   `<data dir>\inbox\<item>` (hex/dash item ids only), at most 180 s, cancelled by a newer clip or
   quit; a complete earlier copy is reused (copy-back never makes "name (1)"). Core keeps the
   relative layout, so `CF_HDROP` lists the top-level entries and folders paste as folders.
7. **Denylist enforcement** in the app (core only stores it): the writer is the clipboard owner
   window's process, read while the clipboard is open; a NULL-owner write falls back to the
   foreground window's process. Exe names compare case-insensitively without ".exe". The
   built-in password-manager list is always applied, the user's list is added on top (as macOS).
8. **Manual mode** = core `clip_mode = Manual`: nothing is published until Win+Alt+C (or the
   tray), received clips wait as "pending" until Win+Alt+V. Not Ctrl+Alt: that is AltGr on many
   layouts (Polish AltGr+C types "ć"), so a Ctrl+Alt hotkey would steal typing. Hotkeys are
   `RegisterHotKey` on the listener window (MOD_NOREPEAT); a clash is reported, not fatal.
9. **Send to**: a shortcut in `shell:sendto` runs `Fsync.exe --send <paths>`; a running
   instance receives the command line through `AppInstance` redirection (same user, same data
   dir) and shows a device picker. Only existing absolute local paths are kept (no UNC, no `\?\`).
10. **History copy-back** works for clips received while the app runs (core keeps their offers
    in memory). The FFI has no call to read a stored item's content, so older items and this PC's
    own copies show Copy disabled. Needs a core API (owner A) for full parity.
11. **One at a time, newest wins.** Publishing and writing each run in their own lane; a change
    or clip that arrives while one is queued supersedes it, and a newer received clip cancels the
    older one's downloads. A replaced entry forgets its streams (a paste already running keeps
    reading). Every task is observed; failures are logged and shown, never unobserved.
12. **Received clips are private on this PC:** our entry carries
    `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0 and
    `CanUploadToCloudClipboard` = 0, so content from another device stays out of Win+V history and
    the cloud clipboard. Our own listener still sees `FsyncOrigin` first (OwnEcho).
13. **Inbox retention** follows the history retention (days, then oldest first until under the
    size limit), at start, daily and when settings change; the clip on the clipboard is kept.
14. **Lifecycle.** Clipboard engine calls (renders, Explorer reads, downloads, publishes) pass a
    gate: on quit new calls are refused, running ones are waited for (bounded), then the engine
    is disposed. Auto-accept of pushed files is capped at 1 GiB (larger offers ask). One engine
    per data folder: core's `engine.lock` failure shows "already running" without retrying, and
    the folder path is normalized for the single-instance key. Received images are bounded
    (16384 px a side, 64 MP) and reserved device names (CON, NUL, COM1...) are refused in paths.
15. **Clipboard open retries.** Another process often holds the clipboard right after our change (RDP clipboard redirection `rdpclip.exe`, clipboard managers). OLE Set, Flush and ClearIfCurrent retry `CLIPBRD_E_CANT_OPEN` 10 x 25 ms (about 250 ms). `ClearIfCurrent` re-checks ownership on each attempt, so an entry the user copied meanwhile is kept. The reader's Win32 open uses 6 attempts, 5 x 50 ms (same budget). Sleeping does not pump messages on the STA (checked on Windows), so the retries cannot re-enter the message loop.

## Consequences
- Lazy paste throughput with core read-ahead (2026-10-15, two app instances on one Windows 11
  x64 machine, loopback direct path, shell copy engine via the Paste verb, `clipboard-with-cli.ps1
  -LazyMiB 200`, done = the copy engine closed the file): 1.5-1.6 s, 125-130 MiB/s, hash ok. The
  core before read-ahead (85f8968), same runs interleaved: 1.5-1.6 s, 126-130 MiB/s. With a near
  zero RTT each 256 KiB round trip is cheap, so read-ahead shows no gain here; its benefit is on
  relay / WAN paths (ADR 010 §6: 256 KiB per RTT), which this machine cannot measure.
  The spike's fake-remote numbers (ADR 010) are unchanged (13.1 s for 200 MiB at 2 ms per read)
  because the spike does not use core.
- Clipboard tests use the real clipboard; test assemblies serialize on a machine-wide semaphore.
- E2E (`apps/windows/tests/e2e/clipboard-with-cli.ps1`) needs an interactive desktop session
  (clipboard, UI Automation, hotkeys); over SSH it runs through a scheduled task with `/IT`.
