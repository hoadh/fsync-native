# Code review: Windows phase 14, round 3

Branch: origin/windows/phase-14, commits 9465cee, 2cb762d, 080a4cf (diff b548b44..HEAD, 12 files).
Method: read-only, git show of branch files, no dotnet on macOS. Windows results taken from the brief
(dotnet test 123/123, e2e 13/13 and 9/9 twice). Line numbers are approximate (Downloads.cs numbered from the file top).

## Score: 9.0 / 10 (threshold 9.5, 0 critical: not met)

Scope: only apps/windows/**, docs/adr/013 (unchanged this round), the phase-14 file. All changed .cs files < 200 lines (max 161).

## Round 2 findings

| Item | Verdict |
|---|---|
| M1 in-flight vs cleanup | Fixed. BeginInFlight is called before `_inbox.Folder()`, ref-counted, released by an idempotent Release. CleanInbox adds `_inFlight.Keys` and the current item; InboxDirectory protects anything younger than 5 min and counts protected bytes in the size budget; cleans are serialized by `_cleanLock`. Residual: see L1. |
| M2 quit off UI thread | Fixed. `_quitting` guard (UI thread only), UI parts disposed first, `Task.Run` for clipboard + engine, `Exit()` after the await on the UI context, all exceptions caught. Residual: see M2-new. |
| M3 Done-event race | Fixed and correct. I walked the interleavings: store-then-recheck in OnTransfer and register-then-TryRemove in Register means at least one side always sees the other; Complete is idempotent via TryRemove. Test runs both orders plus 2000 concurrent. |
| L1/L2 timers after dispose | Fixed. Schedule() no-ops when disposed and swallows ObjectDisposedException; listener released before `_settle`/`_cleanup` are disposed. |
| L4/L5 copy-back, pending | Fixed. CopyBackAsync / PastePendingAsync return the real result; Pending cleared only on success and only if still the same item. |
| L6 CTS leak | Fixed. WriteTicket disposes by whoever comes last (replaced and finished); Token read after dispose cannot happen for the live ticket. |
| L7 empty CF_HDROP | Fixed (InvalidDataException when no top-level entry). |
| L8 StartAsync unobserved, L9 double Accept | Fixed (ContinueWith logger; `_accepting` guard with finally). |
| M4 tests | Real engine, real files, real clipboard-service code, no mocks (FakeEngine untouched, not used). Partly shallow, see M3-new. |

## New defects

### High
**H1. Round-2 refactor of DownloadAsync regressed timeout and cancellation of the start phase.**
`ClipboardService.Downloads.cs` DownloadAsync (~L32-38) now calls `FetchFileTo` for every file first, and only then calls
AwaitDownloadsAsync (~L49-68), which owns the 180 s timeout CTS and the `finally` that cancels transfers. Before, the
try/finally and the timeout covered the loop. Consequences:
1. `fetch_file_to` can block through the dial (the code comments say so, up to ~30 s per file). The 180 s budget starts
   only after the last file started, so N files can wait N x dial time extra; the "bounded paste" guarantee (round 1 item 5) is no longer true.
2. If the clip is superseded or quit starts while a `FetchFileTo` await is pending, the next `ct.ThrowIfCancellationRequested()`
   throws before AwaitDownloadsAsync runs, so already started transfers are never cancelled (and if file 2's `FetchFileTo`
   throws FsyncException, file 1 keeps running too). They keep writing into the inbox folder after the in-flight mark is
   released, and nobody consumes their events (they pile up in `_early`, pruned only by later events).
3. A Done event for file 1 can sit in `_early` while later files dial; pruning at 60 s (OnTransfer) can drop it if another
   event arrives after that, making the paste time out.
Fix: wrap the start loop and the wait in one try/finally with the single timeout CTS created before the loop (pass
`timeout.Token` to the loop), and register each id right after `FetchFileTo` returns (as round 2 did); the finally cancels
every started id still in `_downloads`, including ids that were never registered (register them first, then TryRemove).
Add a test that drives DownloadAsync itself (item the core does not know, cancelled during start).

### Medium
**M2-new. Quit window leaves the 15 s retry timer and events live.** `App.xaml.cs` Quit does not stop `_retry`
(Offline phase). A Tick during the up to ~8 s teardown calls `Bridge.GoOnlineAsync`, which catches only FsyncException;
the engine may already be null, so InvalidOperationException escapes an async lambda and can crash the dying process
(only noise, but unclean). Stop `_retry` first in Quit, and catch InvalidOperationException in GoOnlineAsync.

**M3-new. Tests do not exercise the real DownloadAsync path.** `EagerWaitGivesUpAfterItsTimeout...` and
`ANewerClipCancels...` call AwaitDownloadsAsync with made-up ids. That is why H1 is invisible. The cleanup test
calls BeginInFlight directly; nothing asserts registration happens before the folder exists or that DownloadAsync
releases it on failure. The e2e covers the happy path only.

### Low
- **L1.** `CleanInbox` snapshots `_inFlight` before InboxDirectory enumerates. A copy-back of an old folder (mtime older
  than 5 min, not current) can register just after the snapshot and then be deleted: narrow, and bytes already
  written would vanish mid-download. Compute the keep set inside the Clean lock (pass a Func) or re-check just before TryDelete.
- **L2.** A long-lived folder reused by copy-back keeps its old mtime (no new direct entries), so MinAge does not protect it; only the in-flight set does.
- **L3.** Quit teardown: `Devices/Transfers/Pairing.Dispose()` run before the engine stops; events arriving in that window hit disposed view models (they appear to unsubscribe, not verified).
- **L4.** `WriteTicket.Token` read racing a replace+finish could throw ObjectDisposedException (only `LatestWriteToken`, tests).
- **L5.** L10 from round 2 (lock detection by message text) remains, acknowledged by the author; keep a comment in core next to the string.

## Positive
- The ordering "register in flight before creating the folder" is right; ref-count and idempotent release are tidy.
- WriteTicket is a small, correct two-party disposal.
- Double-check in Downloads is correct and tested under concurrency.
- Quit: guard flag, UI/off-UI split, always reaches Exit().
- Docs/phase file honestly record what was and was not changed.

## Recommended actions
1. Fix H1 (single timeout and finally around start loop + wait) and add a DownloadAsync-level test (cancel during start; timeout counting the start phase).
2. Stop `_retry` in Quit; catch InvalidOperationException in GoOnlineAsync (M2-new).
3. Optional: L1.

## Unresolved questions
- How long does `fetch_file_to` block in practice (dial only, or until first byte)? Decides how bad H1 point 1 is.
- Does the 5 min MinAge also make the daily retention test-sensitive on machines with a very short retention? (retentionDays is clamped to >= 1 in the UI.)

**Status:** DONE_WITH_CONCERNS
**Summary:** All round-2 items (M1-M3, L1, L2, L4-L9, M4) are fixed and the race fix is correct, but the DownloadAsync restructuring introduced a high-severity regression: transfers started before a cancel/failure are never cancelled and the 180 s timeout no longer covers the start phase. Score 9.0.
**Concerns/Blockers:** Below the 9.5 bar until H1 is fixed with a test that drives DownloadAsync; M2-new (stop the retry timer on quit) is a small follow-up. No critical issues.
