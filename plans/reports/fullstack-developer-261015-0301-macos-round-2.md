# Track C macOS round-2 report

Branch `macos/round-2` (pushed, not merged), worktree /Users/alex/Code/fsync-native-wt/macos-round-2. Commits over main 2d87845:
8390e9d test RealBackendRevokeTests; 1cdf80b comments; e3dbc8c merge origin/core/round-2 (baa46dd, Track A); 78fd229 C3 feat; a675bca fix (stale message, echo check).
Review: code-reviewer 9.6/10, 0 critical, 0 major (first pass 7.5 on QA doc labels, fixed).
Tests: swift test FsyncMac 216 run / 18 skipped / 0 failures (baseline 208/14; +8 run incl. 4 new gated skips). FsyncCoreKit 8/8. mac-it script PASS (13 integration tests, incl. new copy-back test). App builds (ad-hoc).

## C1 pending-revoke vs real backend
- Real session NOT obtainable: deployed backend sends OTP by Gmail SMTP, no log/test path (dev-mode log OTP is refused in prod). So no live-session revoke.
- Added `RealBackendRevokeTests` (gated `FSYNC_REAL_API=https://api-fsync.10z.one`, optional `FSYNC_REAL_REFRESH` for an owner token, never printed). Run against staging: PASS. Proves over real TLS: unknown token -> 204 -> `.revoked`, Keychain entry deleted, second launch `.nothingPending`; unreachable host -> `.kept(failures)` then `.dropped` on 5th failure.
- Incidentally observed: while backend returned 502 (down ~03:10-03:12 +07, ~3 min, cause unknown, likely Track D), client mapped it to Network -> `.kept` correctly.
- Full "server session actually dead + 401 on refresh" verified against local backend: `PendingRevokeIntegrationTests` (2 tests) in mac-it PASS. 30 d drop covered by unit test (`testDroppedWhenOlderThanThirtyDaysWithoutCalling`).
- Owner can close the live gap: sign in on the Mac, obtain refresh token, `FSYNC_REAL_REFRESH=... FSYNC_REAL_API=... swift test --filter RealBackendRevokeTests`; or QA row 6.9.

## C2 phase 12 review-medium: FIXED in code
Sync stays off after failed initial settings load: fixed by caed6cc (start clipboard + hotkey on first successful later load, `EngineBridge+Clipboard.clipSettings(changed:)`), b66f1d3 (isQuitting guard, message only dismissed if it is the settings-off one), `ClipSettingsModel.update` re-loads when `!isLoaded`; merged ca6d57d. Tests: `ClipboardStartAfterFailedLoadTests` (pass). Phase file checkbox can be ticked. Same file's third unchecked item (history copy-back) is now done by C3.

## C3 done
- `ClipboardSync.copyBack(itemId:)`: `historyItemReps` off main via EngineCall, written by `ClipboardWriter.writeReps` (all usable MIME: text/html/rtf/png, origin marker, echo guard), message on failure. `HistoryItem.canCopyBack` (all but files) replaces preview heuristic `copyBackText`; HistoryView caption updated.
- Typed lock error: macOS had no message matching; added `EngineAlreadyRunning` text in ErrorText ("fsync is already running on this Mac.", no reset offered) + boot test with second instance (lock is taken at `start()`).
- Integration test: 6000-char text, truncated preview, copy-back restores full text, no echo.

## C4
plans/reports/qa-checklist-261015-macos-manual.md (7 sections, ~60 rows: setup, privacy alert, Mac<->Win pairing, clipboard, ADR 009 file matrix, password managers, history/lifecycle incl. pending revoke, a11y).

## Notes for lead
- Phase 12 todo: tick review-medium and history copy-back; remove "History copy-back restores only 200-char preview" deviation; ADR 009 unchanged.
- Nit not fixed: integration echo check would miss an echo if the engine deduped identical content (unit test covers guard).
- A failed `writeObjects` leaves the pasteboard empty (inherent to NSPasteboard, same as `apply`).

## Unresolved questions
- Did Track D intentionally restart/redeploy staging at ~03:10 (502)? 
- Does the owner want a one-time live-token run (C1 gap) or accept local-backend proof?

## C5 - CI macOS job compile fix
- CI: `macos-15`, no Xcode pin; log shows Xcode 16.4 (MacOSX15.5 SDK, Swift 6.1). Local Xcode 27 can't reproduce CI-only diagnostics.
- `CliEvent`: genuinely Sendable (`CliJSON` enum tree), typed accessors string/int/bool, own file `Integration/CliEvent.swift`.
- `IntegrationTestCase.setUp/tearDown`: no `super` calls (Swift 6.1 rejects them; EngineTestCase overrides neither). Making EngineTestCase @MainActor was tried, rejected (cascade).
- `CliProcess`: exit reported only after both pipes drained; 1 s grace then force-drain (grandchild holding pipes); final no-newline line parsed at EOF; handlers cleared at EOF; teardown terminates all then `awaitExit` (bounded, SIGKILL fallback); `expectExit` throws timeout.
- Hermetic `CliProcessTests` (6) added.
- Local: swift test 222 run / 18 skipped / 0 fail; mac-it PASS. Last commit 89e4c35.
- CI: last executed run 38638772422 macos green (before later refactors). Runs 38640473138/38640564918/38641369532 never started (GitHub billing block). Needs re-dispatch once unblocked.
- Review: 8.5 -> 8.7 -> 9.2 -> 9.4 -> 9.6, 0 critical/high.
