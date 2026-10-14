# Round-2 review: macos/phase-04-12 (HEAD 361568c)

Score: 9.5/10. Critical 0, High 0, Medium 1, Low 4. Gate (>=9.5, 0 critical): PASS, borderline.

## Gates (run in worktree)
- FsyncMac `swift test`: 188 tests, 12 skipped (integration), 0 failures. FsyncMacUITests ran and passed, including OnboardingRenderTests.testDevicesAndMenuRender (H1 fixed).
- FsyncCoreKit `swift test`: 8 tests, 0 failures.
- File sizes: no changed Swift file is 200 lines or more. EngineBridgeEventsTests.swift (214) is pre-existing and untouched.

## Round-1 verification
- H1 verified. Device row padded in the test; UI target green.
- H2 verified. ClipboardSync.swift:114-126 keeps one queued request (newest wins) and re-runs after the current one. `queued` is cleared on stop (l.64). `publishing` resets when the loop exits, including after stop mid-flight. Tasks created from `changed()` run FIFO on the main actor. A request dropped by a stale `expected` is always followed by the watcher's own request for the newer count, so no lost wakeup.
- H3 verified. ClipboardSync.swift:131, 140, 143 check changeCount against the value the filter judged: before the snapshot, after it, and after the off-main build. Residual: the frontmost bundle id is sampled at poll time (inherent, acceptable).
- H4 verified. AppDelegate.swift:29 calls prepareToQuit synchronously via MainActor.assumeIsolated. applicationShouldTerminate runs on main, clearContents is non-blocking, and clearIfOurs is idempotent (FileClipWriter.swift:80, ClipboardWriter.swift:99). No deadlock: it never waits on a main-queue block, and shutdown stays in a Task.
- M5/M6 mostly verified. Load is off main via EngineCall, edits are refused until isLoaded, and a failed load keeps sync off with a message. See M-new-1.
- M7 verified for TIFF to PNG conversion and oversize TIFF refusal (RepresentationMapper.swift:38-46). See L-new-2 for the residual.
- M8 verified. Cleanup runs at ready, on retention change (EngineBridge+Clipboard.swift:29), and daily. `clean` applies day cutoff and byte cap, keeping the current item.
- M9 verified. Copy-back is offered only when the preview equals the full text (HistoryModel.swift:8-11).
- M10 verified (FilesSync.swift:62). Cancel breaks the offer and is not counted as failure.
- M11 verified. Offers capped at 20 (FilesSync.swift:51); auto-accept stops at 1 GiB (l.48).
- M12 verified (BlockingFetch.swift:27-29, FileFetchService.swift:53). Spins in currentMode, falling back to .default. Main-queue events are serviced in common modes (tracking, modal included). No practical busy-spin. Paste timeout 180 s.
- M13 verified. HotkeyManager has a nonisolated deinit that releases hotkey and handler; a failed register removes the handler it installed.
- L14-L17 verified. TransfersModel keeps ended outcomes (bounded to 200), reset resolves waiters as cancelled, HistoryModel.reset clears isLoading, acceptPending/copyBack go through `serialized` (ClipboardSync+Incoming.swift:9-14).

## Medium
- M-new-1. A failed initial settings load is never retried into a running sync.
  - Where: EngineBridge+Clipboard.swift:46-58 and ClipSettingsModel.swift:47-50.
  - After a failed load, startClipboardWhenSettingsAreKnown returns without clipSync.start() or hotkey.register(). A later successful update() auto-load fires onChange -> clipSync.apply, but nothing calls start(). Sync stays off until restart or until phase leaves and re-enters .ready, and the "sync is off" message goes stale.
  - Fix: in clipSettings(changed:), if phase == .ready and clipSync is not running, run the start sequence (start, register hotkey, reload history, inbox cleanup). Or add an explicit retry action.

## Low
- L-new-1. Quit window (EngineBridge+Clipboard.swift:76-79, AppDelegate.swift:29). prepareToQuit clears the entry, but clipSync keeps running until removeClipboardModels after shutdown(), which can take seconds. A clip arriving then can re-write a lazy entry that dies on exit. Fix: call clipSync?.stop() inside prepareToQuit before clearing.
- L-new-2. RepresentationMapper.swift:61-66. snapshot still copies full text/HTML/RTF/PNG bytes on main before the maxClipBytes check, and fileExists runs on main. Only the conversion moved off main. A huge clip costs one main-thread stall.
- L-new-3. ClipSettingsModel.swift:46-68. update is not serialized: two quick edits each copy `settings` before the await, so the second can overwrite the first. Serialize them, or re-apply the edit on the current settings after the await.
- L-new-4. InboxDirectory.swift:55. trim can delete the folder of an in-flight eager download that is not yet currentItemId. Rare (fresh mtime, cap typically 1 GiB or more). Add the in-flight item id to `keeping`.

## Positive
Targeted tests exist for each fix (stale count, change during read, second copy during publish, byte cap, waiters after eviction). The ClipboardSync+Incoming.swift split keeps files small. The `expected` changeCount design is simple and effective.

## Unresolved questions
- Does a global hotkey count as user-originated pasteboard access under macOS privacy enforcement? (needs a GUI session)
- The lazy provider wait does not work when the read is issued from a main-queue block inside our own app (documented limitation).

**Status:** DONE_WITH_CONCERNS
**Summary:** All round-1 findings (H1-H4, M5-M13, L14-L17) are fixed and verified, and both suites are green (188/0 FsyncMac including UI tests, 8/0 FsyncCoreKit). Score 9.5/10, 0 critical, 0 high, which meets the gate.
**Concerns/Blockers:** One medium: sync and the hotkey never start after a failed initial settings load, even when a later load succeeds (M-new-1). Four low items (L-new-1 to L-new-4). None blocks merge.
