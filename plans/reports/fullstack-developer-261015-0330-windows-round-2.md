# Windows round 2 (Track B): B1-B3 done, B4 pending Track A

Branch windows/round-2 (pushed, not merged), base main 2d87845.

## Done
- B1: SecretStore pending revoke set in memory only after successful Save (MutateAndSave rollback, pending-revoke only; SaveSession/ClearSession deliberately unrolled: rotated token stays newest, sign-out clears memory). Flaky WithoutAnEngineSignOutRevokesDirectly: lock + All==ref-1. Cosmetics: trailing space, unused using. Test FailedSaveLeavesNoPendingRevokeInMemory.
- B2: CancelStartedTransferTests (5): caller cancel after start, budget timeout, late-start return, failing 2nd start cancels 1st, finished transfer not cancelled. Real DownloadAsync over scripted IFsyncEngine (FakeEngine got FetchFileTo/Cancel). Pins our bookkeeping only; core cancel (partial file cleanup) still e2e only.
- B3: L1 eager branch in Task.Run; L2 Dispose cancels ticket before closing gate; plus IsDisposed guard in WriteAsync (early + after ticket swap).

## Verification
- Windows dotnet test: 9 + 47 + 94 = 150/150 (baseline 144 + 6 new). Flaky subset (PendingRevoke|CancelStarted, 26 tests) 15x green.
- Review: 9.5/10, 0 critical/high (rounds: 8.5, 9.0, 9.5).

## B4 pending
origin/core/round-2 does not exist yet (only origin/core/hardening). Not started.

## Unresolved
- Add test pinning SaveSession/ClearSession no-rollback contract (reviewer suggestion, optional).
- Windows worktree C:\Users\dev\workspace\fsync-round2 uses CARGO_TARGET_DIR of main checkout.

## B4 (done)
- Merged origin/core/round-2 (baa46dd) into windows/round-2 (merge d716737); bindings regenerated on waso (scheduled task), DLL rebuilt; Generated/fsync_ffi.cs LF.
- History Copy: items not received this session load full content via HistoryItemReps (eager <=256KB, lazy above, unconvertible reps skipped, a newer write cancels the wait); CanCopyBack/CopyBackAsync take HistoryItem; files-only only if offered this session. WriteAsync refactored to WriteCoreAsync (file ClipboardService.HistoryCopy.cs new).
- engine.lock: typed catch FsyncException.EngineAlreadyRunning; string matching removed; UserMessages case. Round1 real second-engine test passes.
- dotnet test waso: 9+47+99 = 155/155; new tests 10x green. Review 9.5, 0 critical/high/medium.
- Gotcha: regenerating Generated/fsync_ffi.cs in fsync-round2 dirties the worktree and blocks later checkout --detach; `git checkout -- <file>` first.
