---
owns: ["apps/macos/FsyncMac/Sources/FsyncMac/AppDelegate.swift", "apps/macos/FsyncMac/Sources/FsyncMacUI/Notifications/**", "apps/macos/FsyncMac/Tests/FsyncMacUITests/IncomingFilesNotifierTests.swift"]
dependsOn: [04]
---

# Phase 05: macOS incoming-file notifications (this Mac)
<!-- red-team #5: split out of old phase 04 -->

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` §Notification rules, §Sanitizer
- Phase 02: `bridge.incomingFiles` (sanitized, coalesced, ordered notices), `bridge.navigation`, `bridge.windowRequest`; phase 04: `WindowIDs` (`Shell/WindowIdentifier.swift`)
- Current: `Sources/FsyncMac/AppDelegate.swift` (AppRuntime, closeLaunchWindow by title)

## Overview
- Priority: P2. Status: completed except manual smoke (commit 9ac43bb, fix 191bf98, pushed to origin). Manual smoke pending owner.
- Deliver `UNUserNotificationCenter` notifications for incoming files. Arrival never activates the app; a click opens the Transfers page. Also moves `closeLaunchWindow` to identifier lookup and updates the macOS 13 comment.

## Key Insights
- `UNUserNotificationCenter.current()` crashes without a bundle (`swift run`). Guard: only when `Bundle.main.bundleURL.pathExtension == "app"`; otherwise the notifier is a no-op (tested).
- The delegate must be set in `applicationDidFinishLaunching`, so a click on a notification delivered while the app was running routes correctly. AppDelegate (owned here) creates `IncomingFilesNotifier(bridge: AppRuntime.bridge, center: ...)` and keeps it in `AppRuntime`.
- No activation on arrival <!-- red-team #10 -->: the notifier only calls `center.add(request)`. It never calls `NSApp.activate`, `openWindow`, or `windowRequest`. Only `didReceive response` (explicit click) does `navigation.show(.transfers)` + `bridge.windowRequest.send()`.
- Suppression: skip when `NSApp.isActive` and the key window identifier is `fsync.main` and `navigation.section == .transfers`.
- Coalescing and sanitizing already happen in core (phase 02). The notifier uses a stable `threadIdentifier` per sender and a request id per coalesced window, so updates replace rather than stack.
- Authorization is requested lazily at the first notice (`.alert` only, no sound/badge).

## Requirements
- `Sources/FsyncMacUI/Notifications/IncomingFilesNotifier.swift`: subscribes to `bridge.incomingFiles`. Protocol `NotificationPosting` wraps the center, so tests use a fake. It applies the suppression rule and implements `UNUserNotificationCenterDelegate` (`willPresent` → banner only if not suppressed; `didReceive` → route).
- `AppDelegate.swift`: create the notifier behind the bundle guard. `closeLaunchWindow()` closes by `WindowIDs.main` identifier (fallback to title only if the identifier is not yet set on the first runloop). Update the comment: target 14, `.suppressed` is 15+. <!-- red-team #14 -->
- Body text strictly from `IncomingFilesNotice.title/body`; never file names or paths in `userInfo` (only a constant action key).

## Architecture
```
FilesSync.receive → coalescer → bridge.incomingFiles
   → IncomingFilesNotifier: suppressed? drop : authorize-once → post(title, body, thread = sender)
click → didReceive → navigation.show(.transfers) → bridge.windowRequest.send() → FsyncMacApp.showMain()
```

## Related Code Files
- Modify: `apps/macos/FsyncMac/Sources/FsyncMac/AppDelegate.swift`
- Create: `apps/macos/FsyncMac/Sources/FsyncMacUI/Notifications/IncomingFilesNotifier.swift`, `.../Notifications/NotificationPosting.swift`, `apps/macos/FsyncMac/Tests/FsyncMacUITests/IncomingFilesNotifierTests.swift`
- Delete: none

## Implementation Steps
1. `NotificationPosting` protocol + UN adapter.
2. Notifier: subscribe, suppression, lazy authorization, delegate routing.
3. AppDelegate wiring + identifier-based `closeLaunchWindow` + comment.
4. Tests with a fake poster:
   - no-bundle → no-op;
   - suppressed when Transfers is visible and active;
   - a notice posts with the exact spec text;
   - a click routes to `.transfers` + one windowRequest;
   - arrival produces zero windowRequest/activation calls.
5. `swift build && swift test`. Manual: `build-fsync-mac-app.sh` bundle, send files from `fsync-cli`; one notice, click opens Transfers.

## Todo List
- [x] NotificationPosting + adapter
- [x] IncomingFilesNotifier (suppress, authorize, route)
- [x] AppDelegate wiring + identifier close + comment
- [x] Notifier tests
- [ ] Manual bundle smoke: OPEN, owner. Pending: cold-launch click opens Transfers; 3 offers from one sender → one notice.

## Success Criteria
- `cd apps/macos/FsyncMac && swift build && swift test` green, including `IncomingFilesNotifierTests`.
- `grep -n 'title == "fsync"' apps/macos/FsyncMac/Sources/FsyncMac/AppDelegate.swift` is empty or only in the documented fallback.
- Manual (bundle build): 3 offers from one sender within 10 s → one notification showing the count; no app activation until clicked.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Crash in `swift run` | H x H if unguarded | Bundle guard + no-op test |
| Notification steals focus | L x M | No activate on arrival; test asserts it |
| User denies authorization | M x L | Silent no-op; tray/popover still show offers |

## Security Considerations
- Text = sanitized sender + count + size. `userInfo` holds only a constant action key; no names or paths. Notification Center persistence therefore leaks no file names.

## Next Steps
- Phase 06 (optional) may extend AppDelegate under the handoff rule. Rollback: revert phase commits; core notices are emitted but unused.
