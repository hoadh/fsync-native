# Phase 03 — Linux -> Windows lazy file latency (8.7 s vs 10 s gate)

## Context
- X3 app→win file: 4577 ms (run 1217) and 8701 ms (run 1241), against a 10 s gate. ADR 014 "Open" says the variance comes from the Win+Alt+V poll (1.5 s steps) and PasteHelper.
- Harness: `apps/linux/tests/e2e/lib-cross-os-gui-legs.sh:263` (`gui_in_lazy_leg`), `apps/windows/tests/e2e/cross-os-gui-peer.ps1`.
- Windows app: virtual-file paste (`apps/windows/src/...` ClipboardService inbound, lazy file data object).

## Overview
- Priority P3. Estimate 1.5 d. First split the time: is it the product (lazy fetch on paste), or the harness (poll step, PasteHelper start)?

## Steps
1. Add timestamps per stage to the in-lazy leg (Windows side, JSON fields, all on the Windows clock, so no skew):
   - clip received;
   - paste triggered;
   - fetch start;
   - fetch end;
   - file on disk.
2. Run X3 three times (waso-vm `taskMode` interactive, user logged on over RDP; restore headless after).
3. Fix where the time goes:
   - **harness:** poll at 250 ms instead of 1.5 s steps, and keep PasteHelper warm;
   - **product:** prefetch small files (≤ 1 MiB) when the clip arrives, or begin the fetch on the first `GetData` instead of on the copy-engine request.
   Product changes need their own Windows unit tests.
4. **X2/X5 text app→win:** this leg can fail on Windows peer start-up, not on transfer. It measured 6256 ms on 2026-11-11 and 4132 ms in run 1511. Copy #1 is made as soon as the Windows `remote run` is launched. If the CLI peer is not online yet, the first send fails, and the retry (about 2 s + 5 s) ends up in the measured latency. Fix the harness so the first copy waits for a ready signal from the peer, for example the app's device-online status for the Windows device, and record which copy index arrived. 2026-11-11: main (4e7b33b) shows the same delay, 6238 ms (skew 6157 +-72 ms, checked over ssh), so it was not caused by phase 01. Two candidates: harness start-up, or the offline-peer publish change from PR #1 (publish wait 1.5 s, first dial 2 s + 5 s retry). Find out which before changing anything.
5. Gate: 5 X3 runs, median ≤ 5 s and max < 10 s. Record the results JSON.

## Todo
- [x] Stage timestamps in the leg (X3 lazy + X2/X5 text)
- [x] 3 measuring runs, breakdown in the report (reports/implementer-*-phase-03.md)
- [x] Fix (harness and product) + tests
- [x] 5-run gate, results JSON, ADR "Open" row updated
- [ ] Review >= 9.5, PR, CI green

## Risks
Needs the user's RDP session. If waso-vm is off, stop and ask (do not switch to headless for GUI legs).
