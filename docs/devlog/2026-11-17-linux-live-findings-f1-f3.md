# Linux LIVE findings F1-F3: duplicate Ghostty copy, stale tray state, stale Omarchy bind help

**Date**: 2026-11-12 found; fixed and merged 2026-11-17 00:49 (PR #7, `ee3f393`)
**Severity**: Low
**Component**: `apps/linux/platform/src/sync` (auto-copy dedupe), `apps/linux/app/src/engine/tray.rs`, `shortcut_help.rs`
**Status**: Resolved (F1 and F2 re-checked live on `d6228e0`; one unexplained observation)

## What Happened

Live checks on Hyprland (Omarchy) found three problems. F1: every Ctrl+Shift+C in Ghostty reached the peer as two clips about 2 ms apart. F2: the tray "Pause sync" check mark did not change on Omarchy's tray, so a user could click it expecting "pause" and unpause. F3: the hotkey help told Omarchy users to edit `hyprland.conf`, but current Omarchy uses `~/.config/hypr/bindings.lua`.

The first F1/F2 fix (`fe2db13`) was reviewed at 6.5/10 with H1 (a copy could be silently dropped) and H2 (the tray never refreshed outside a click). Four more commits followed (`13bc7a1`, `baaf93f`, `d6228e0`, `22f68d3`). The final review was 9.5 and the live re-check passed.

## Impact

Duplicate clips on every peer for each Ghostty copy, and a pause control that could do the reverse of what the user intended. Both are low severity, but both were visible on the main daily path.

## Technical Details

- F1 dedupe: an automatic change equal to the last automatic change within `AUTO_REPEAT_WINDOW` (1 s) is dropped. Explicit sends always publish. Test-only setter widens the window in engine tests.
- The first live re-check of `baaf93f` still sent the copy twice, 1.5 s apart. Cause: the window was measured at admission, but the second sighting waited in the "newest wins" lane while the first publish waited about 1.5 s for confirmations. Fix `d6228e0`: the watcher stamps the time of sighting, and the window is measured from that.
- Review H1 (`13bc7a1`): the fingerprint was recorded and then the publish was cancelled, so the second copy was dropped and nothing was sent. The cancel check and the record now share one lock.
- F2 root cause: ksni re-reads `Tray::menu()` only on a row click or `Handle::update`, and the app never called `update`. Omarchy also kept a stale check mark on `ItemsPropertiesUpdated`. Fix: each state has its own row count, and every status change calls `Handle::update`.
- F3: separate entries for Omarchy (`o.bind("SUPER + ALT + C", ...)` in `bindings.lua`) and Hyprland (`bind = SUPER ALT, C, ...` in `hyprland.conf`).

## What We Tried

- Dedupe at admission time (`baaf93f`). It passed the tests and failed live. Tests used a timing window that a slow publish could stretch.
- Check mark only. Rejected: Omarchy's tray ignores the property update, so the row count had to change.

## Root Cause Analysis

Two assumptions were wrong. (1) "A duplicate is a copy within 1 s of the last publish" was measured at the wrong point, so a slow publish path widened the gap. (2) "Every state change reaches every tray host" assumed a push model that ksni does not implement. The review questioned the stated F2 cause (the Quickshell source it cited may not match the installed 0.3.2). That cause is still unproven; the live check confirmed the row-count fix, not the explanation.

## Key Decisions

- Drop the duplicate rather than change the copy path: the apps cannot tell Ghostty's double set from two real copies.
- Explicit sends and received clips reset the dedupe record, so a user's deliberate send is never dropped.
- Trade-off: a held clip replaced by another only changes a label, which may stay stale on Omarchy (minor).

## Lessons Learned

- Time-window logic must measure from the same event the window is defined on (here: the sighting). Test with the lane delay in the loop, not only with an idle queue.
- Live re-checks found what the reviews did not. Keep the re-check step in the plan, with the same steps as the first run.
- Do not put a root-cause claim about a third-party tray into a module doc until it is verified on that host; mark it as a hypothesis.

## Next Steps

- Watch for a fourth duplicate: once, tray or bind sends of one clip arrived 4 times in 0.7 s. Not reproduced (owner: Alex; add a log line with the sighting count if it recurs).
- Password-manager filter check on Hyprland is still deferred (no manager installed on the test host).

## Unresolved Questions

- Quickshell 0.3.2 and its handling of `ItemsPropertiesUpdated` is not settled; the live fix does not depend on the answer.
