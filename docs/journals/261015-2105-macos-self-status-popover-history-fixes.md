# 2026-10-15 21:05 — macOS: self status, popover anchor, history refresh

Owner hand-tested both apps on one LAN and reported three Mac bugs.

## What broke and why
- **"This Mac · Offline"**: the core marks `is_self` devices offline because there is no connection to self. Windows already hid status ("This PC"); the Mac row didn't. Fix: `DeviceRowView` hides the dot and label for self.
- **Popover far below the icon**: the `MenuBarExtra(.window)` window resizes from its bottom-left origin, so the top drops when content shrinks. Fix: `MenuWindowTopAnchor` (NSView background) restores the top edge after resizes that kept the origin. Observers use `queue: nil` so the fix lands in the same post, before draw.
- **Empty history on Mac**: `HistoryModel` loaded once at clipboard start, when the list was empty, and SwiftUI keeps a closed window's view alive, so `.task` never reran. Fix: `EngineBridge.refreshHistory()` on remoteClip, incomingFiles, `ClipboardSync.activity`, `FilesSync.onSent` and when the window opens. `HistoryModel.refresh()` coalesces calls in one turn, keeps the loaded row count (the engine caps a read at 200) and skips if a reset ran meanwhile. The core already writes history before emitting events (`clip/receive.rs`).

## Verification
- 9 new regression tests. Each fails without its fix.
- Core 228 tests (18 skipped, 0 failed), UI 9/9.
- Review round 1: 8/10. Round 2: 9.5/10 after coalescing and synchronous observers.
- Live check on the signed build:
  - The self row reads "alexpro, This Mac" with no status.
  - A clip copied while the history window was open showed up without reopening it.
  - The popover sits at y=35.

## Lessons
- AX `whose title is` was flaky for the history window; `whose name is` works.
- A `Task` started from a test runs after the next synchronous call. Yield once before testing an interleaving.

## Open
- The shrink case was not reproduced live; unit tests cover it.
- History rows show a raw node id instead of the device name.
- Two test strings (`fsync-history-*`) are now in clipboard history on both machines.
