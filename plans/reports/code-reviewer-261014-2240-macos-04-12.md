# Code review: macos/phase-04-12, round 1 (c82e4dd)

Score 8.3/10. 0 critical, 4 high. Gate not met; sent back. Round 2: see code-reviewer-261014-2310-macos-04-12-round-2.md (9.5).

## High
1. Red UI test OnboardingRenderTests.testDevicesAndMenuRender (row height 34 < 50).
2. ClipboardSync: rapid copies dropped while `publishing` is true.
3. Privacy TOCTOU: filter decided at poll changeCount, pasteboard read later.
4. prepareToQuit never called.

## Medium
5 sync engine.settings() on main; 6 failed settings load overwrites stored settings; 7 pasteboard read/TIFF->PNG on main before size check; 8 inbox cleanup only at ready, retentionBytes unused; 9 copy-back writes 200-char preview; 10 accept continues after cancel; 11 offers unbounded, auto-accept no size cap; 12 nested run loop .default only, 600 s; 13 HotkeyManager no deinit.

## Low
14 explicit send skips app denylist (document); 15 bundle id logged public; 16 TransfersModel stranded waiters, HistoryModel.reset isLoading; 17 acceptPending/copyBack not serialized.
