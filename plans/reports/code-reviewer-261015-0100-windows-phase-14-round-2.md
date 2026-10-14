# Code review: origin/windows/phase-14, round 2

Score 9.2/10. 0 critical, 0 high. Below 9.5 merge bar. Read-only on Mac; Windows tests from brief (dotnet 117/117, e2e 13/13 + 9/9).

## Round 1 items
1 denylist merge: fixed. 2 inbox cleanup / "name (1)": fixed, gap M1. 3 Win+V/cloud flags: fixed. 4 serialized write/publish: fixed (2 lanes, generation counter). 5 eager timeout + 20 MiB: fixed. 6 quit/engine/lock/DataDir: mostly fixed, see M2. Follow-ups all done. Scope clean, files < 200 lines, new tests use real engine.

## Medium
- M1 `ClipboardService.Lifecycle.cs:21`, `InboxDirectory.cs:33-45`: cleanup protects only current item; in-flight download folder can be deleted (share-delete on Windows). Fix: in-flight id set passed as keep; skip recently written folders; lock CleanInbox; test.
- M2 `App.xaml.cs:138`: quit disposes clipboard (<=3 s) + bridge (<=5 s) on UI thread. Run off UI thread then Exit(), or document in ADR 013 §14.
- M3 `Downloads.cs:36/60`: race between GetOrAdd+_early.TryRemove and OnTransfer storing to _early -> lost Done event, 180 s hang. Fix: re-check _downloads after writing _early.
- M4: no tests for newest-wins cancel, eager timeout, M3 race, cleanup vs in-flight, publish supersede.

## Low
L1 `_settle.Dispose()` before listener dispose (Timer.Change on disposed timer in wndproc). L2 `_cleanup.Change` after Dispose. L3 cancel in finally outside gate at quit (logged). L4 `CopyBackAsync` returns true when WriteAsync false. L5 `PastePendingAsync` clears Pending before write. L6 replaced CTS not disposed. L7 `EagerFiles.TopLevel` all-dropped -> empty CF_HDROP. L8 `_ = Clipboard.StartAsync()` unobserved; only FsyncException caught. L9 Accept double-click reentrancy (`TransfersViewModel.cs:83`). L10 lock detection by message text "already running" (`EngineBridge.Lock.cs:16`); comment in core.

## Unresolved questions
- Timer.Change on disposed timer throws on target runtime? (decides L1 severity)
- Tell user when Eager falls back to lazy for >20 MiB?
- Manual checklist open: Explorer paste, cancel mid-copy, Office/Chrome, password manager, ARM64, Win10.

**Status:** DONE_WITH_CONCERNS
