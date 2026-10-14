# Code review: Windows phase 14, round 4

Branch: origin/windows/phase-14, commits caeeaaf, 4d0145e, f7f5fe5 (diff 080a4cf..HEAD: 7 files).
Method: read-only (git show/diff), no dotnet on macOS; Windows results (124/124, e2e 13/13 + 9/9 twice) taken from the brief.
Scope: vs merge-base with origin/main the branch still touches only apps/windows/**, docs/adr/013, the phase-14 file and its report. Files < 200 lines (Downloads.cs 123).

## Score: 9.6 / 10 (threshold 9.5, 0 critical, 0 high): meets the bar

## Round 3 findings

| Item | Verdict |
|---|---|
| H1 one budget, one try/finally over start loop + wait | Fixed. DownloadAsync (ClipboardService.Downloads.cs ~L31-75): linked CTS with CancelAfter(EagerTimeout) created before the loop; `start.WaitAsync(budget.Token)` makes the start phase cancellable and bounded; the single `finally` cancels every id still in `_downloads`. Both exits are right: an OCE with `ct` not cancelled becomes TimeoutException, caller cancel stays OCE. A failed transfer (WhenAll throws) cancels its siblings; a FetchFileTo FsyncException cancels earlier ids. AwaitDownloadsAsync is gone (no dead code). |
| Ids registered right after FetchFileTo | Fixed. `waits.Add(Register(ids[^1]))` follows `ids.Add` with no await between, so no id is outside both lists; Register consumes `_early`, so an outcome that arrived during later dials is handed over (the old 60 s prune window no longer matters). |
| Late-start cancel | Correct. When we stop waiting, CancelOnceStarted attaches OnlyOnRanToCompletion to the still-running start task and cancels the id when it returns (immediately if it already did); CancelTransferAsync swallows and logs failures, enqueues the id for diagnostics. |
| Quit: retry stop + phase ignore | Fixed. `_retry?.Stop()` is the first action of Quit (UI thread, after the `_quitting` guard); OnPhaseChanged returns early while quitting, so Offline cannot restart the timer nor Ready restart the clipboard. |
| GoOnlineAsync InvalidOperationException | Fixed (logged, no phase change). Slightly broad, acceptable. |
| In-flight marker under cleanup lock | Fixed and sound. BeginInFlight takes `_cleanLock` via Exclusive; Clean reads the kept set inside the same lock. A clean either finished before the mark (and cannot see the folder yet, or sees it recent via Touch) or runs after it and sees the mark. Order in DownloadAsync (mark, Folder, Touch) is right. |
| mtime refresh on reuse | Fixed (Touch after Folder; failures ignored). Test asserts it. |
| Tests drive real DownloadAsync | Yes. Spent budget (TimeSpan.Zero) throws TimeoutException before any fetch and releases the mark (clean removes the aged folder); older ticket token gives OCE with CancelledTransfers empty (no transfer started for a superseded clip); an unknown file makes core fail, reused file kept, folder touched, then released. Real engine, real file system, no mocks. |

## New / remaining defects

### Medium
None.

### Low
- **L1. Inline blocking on the caller thread.** WriteAsync awaits `_inLane.WaitAsync().ConfigureAwait(false)`, which completes synchronously when the lane is free, so on a UI-initiated write (history Copy, Win+Alt+V, tray) the eager branch starts DownloadAsync on the UI thread: BeginInFlight now waits for `_cleanLock` (a running clean enumerates the whole inbox, possibly seconds on a big one) plus the existing Folder/Touch/Existing file IO. Fix: wrap the eager branch in `Task.Run` (or `await Task.Yield()` before it).
- **L2. Gate-closed race at quit.** Dispose closes `_gate` before `_inTicket.Cancel()`; a start in that gap throws OperationCanceledException with `ct` not yet cancelled, so it is reported as TimeoutException ("took too long") during quit. Cosmetic; cancel the ticket first.
- **L3.** `start` faulting after we stopped waiting is never observed (continuation is OnlyOnRanToCompletion): harmless in .NET Core, add a faulted logger for tidiness.
- **L4.** Retry stop in Quit is on the UI thread only; a Tick already dispatched could still run, now harmless thanks to the InvalidOperationException catch.

### Gap judgment: no unit test for cancelling an already-started transfer
Acceptable for merge, with a caveat. A unit test needs a started core transfer, which needs a real peer offer; faking core is disallowed, and a two-engine pairing test inside the unit project would be heavy. The remaining untested lines are the finally loop and CancelOnceStarted (about 10 lines, reviewed above and correct) on top of tested pieces (Register/Complete race, budget, cleanup release). The e2e eager step covers the happy path only, so the cancel path has no automated coverage; this is exactly where round 3 regressed. Recommended follow-up (not blocking): one e2e step that starts an eager paste of a large file, copies a newer text clip, and asserts the app log shows the transfer cancelled and no `.part`/file left growing in the inbox.

## Positive
- The H1 fix is smaller than the code it replaced, with one budget, one cancellation and one cleanup point.
- Lock-scoped mark/keep-set removes the cleanup race by construction rather than by timing.
- Tests assert behaviour through the public eager path and the real cleanup, not internals.
- Phase file records exactly what is and is not covered.

## Recommended actions
1. Optional before merge: L1 (move eager branch off the caller thread), L2 (cancel ticket before closing gate).
2. Follow-up: e2e step for cancel-while-downloading.

## Unresolved questions
- Does core's `cancel` remove the partially written destination file for an eager transfer, or only the `.part` in the store? Decides whether a cancelled download leaves anything in inbox\<item> (cleanup removes it by age either way).
- Manual checklist still open (real Explorer paste, Office/Chrome, a real password manager, ARM64, Windows 10).

**Status:** DONE
**Summary:** The round-3 High and all listed items are fixed correctly (one budget and one try/finally, ids registered at once, late-start cancel, quit retry stop, lock-scoped in-flight mark, touch on reuse); the new tests drive the real DownloadAsync on the real engine. Score 9.6, no critical or high findings.
**Concerns/Blockers:** None blocking. The cancel-of-a-started-transfer path has no automated test (accepted given the core-mock ban; add an e2e step later). Low items L1 (eager branch may block the UI thread on the cleanup lock) and L2 are cheap to fix.
