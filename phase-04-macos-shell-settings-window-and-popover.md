---
owns: ["apps/macos/FsyncMac/Sources/FsyncMac/FsyncMacApp.swift", "apps/macos/FsyncMac/Sources/FsyncMac/SettingsOpener.swift", "apps/macos/FsyncMac/Sources/FsyncMacUI/*.swift", "apps/macos/FsyncMac/Sources/FsyncMacUI/Shell/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/Settings/**", "apps/macos/FsyncMac/Sources/FsyncMacUI/Menu/**", "apps/macos/FsyncMac/Tests/FsyncMacUITests/ShellRenderTests.swift", "apps/macos/FsyncMac/Tests/FsyncMacUITests/MenuWindowTopAnchorTests.swift"]
dependsOn: [03]
---

# Phase 04: macOS shell, Settings window, popover restyle (this Mac)
<!-- red-team #5: split from old phase 04; notifications → phase 05, delight → phase 06 -->

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md`
- Research: `research/researcher-261019-macos-native-ui-patterns.md` (openSettings workaround, MenuBarExtra limits)
- Current: `Sources/FsyncMac/FsyncMacApp.swift` (`showSettings` sendAction, `showMain` title lookup, `showHistory`), `Sources/FsyncMacUI/{RootView,MenuPopover,ReadyView,TrustGate,SignOutButton}.swift`, `Menu/*`, `Settings/*`
- Phase 02: `bridge.navigation` (section `MainSection?`); phase 03: `DevicesPageView`, `HistoryView`, `TransfersPageView`, `PairingSheet`

## Overview
- Priority: P1. Status: pending.
- Task 0 is a blocking spike (openSettings). Then: main `Window` with NavigationSplitView + gating, pairing sheet, history window removed, Settings window, restyled popover, window lookup by identifier.
- Not here: notifications (05); CommandMenu, ActivationPolicyController, symbolEffect, icon states, Quick Look (06). Liquid Glass / GlassChrome deleted entirely. The sidebar/toolbar adopt the system look automatically when built with a newer SDK. <!-- red-team #6 -->

## Key Insights
- **Spike (task 0, gates every other step)** <!-- red-team #4 -->:
  - Mechanism: a hidden `Window(id: "settings-opener")` declared BEFORE `Settings`. It holds `@Environment(\.openSettings)` and listens for a `SettingsRequest`.
  - The opener window is never closed; it stays alive but invisible. An NSViewRepresentable configures its NSWindow: `alphaValue = 0`, `ignoresMouseEvents = true`, borderless (cannot become key/main), `isExcludedFromWindowsMenu = true`, `collectionBehavior = [.transient, .ignoresCycle]`, 1x1 frame offscreen.
  - Pass criteria on macOS 26 (macOS 14 not tested: user accepted the risk, Validation 1; record it in the QA checklist): <!-- Updated: Validation Session 1 - macOS 14 risk accepted, test on 26 only -->
    Settings opens in front and key in 5/5 tries (a) from the popover with no window open, (b) ⌘, from the main window, (c) after closing and reopening. The opener is never visible, never in the Window menu or the ⌘` cycle, never key.
  - Record whether a temporary `.regular` activation policy is required. If yes, use it (Dock icon approved, Validation 1).
  - Fail → STOP the phase, report, and ask the user (option: Settings as an in-window page). `NSApp.sendAction(showSettingsWindow:)` is NOT accepted as a sufficient fallback. Results go to `reports/macos-settings-spike-<date>.md`.
- `.defaultLaunchBehavior(.suppressed)` is macOS 15+: `AppDelegate.closeLaunchWindow()` stays (AppDelegate is owned by phase 05; until then it closes by title "fsync", which leaves the opener window alone because it has no such title).
- Window lookup by `NSWindow.identifier`, not title <!-- red-team #14 -->:
  - `Shell/WindowIdentifier.swift`, an NSViewRepresentable, sets `fsync.main` / `fsync.settings-opener` on the hosting window.
  - `showMain()` finds `fsync.main`. A shared `WindowIDs` enum goes in the same file (phase 05 reuses it).
- `MenuBarExtra(.window)` cannot be closed programmatically; actions that open the main window just activate it.
- `SignOutButton` is used by the popover, Settings and sidebar: keep one view.
- `List(selection: $navigation.section)` binds `MainSection?` (phase 02 decision); detail maps `nil` → Devices. <!-- red-team #7 -->

## Requirements
- Scenes (`FsyncMacApp.swift`):
  - `MenuBarExtra(.window)` with the label unchanged (current symbol logic).
  - Settings opener `Window`; `Window("fsync", id: "main")` with `.defaultSize(760,520)` and `.windowResizability(.contentMinSize)`; `Settings`.
  - Remove `Window(id: "history")`; `showHistory` → `navigation.show(.history)` + `showMain()` (refresh happens in the bridge sink). <!-- red-team #14 -->
- `RootView.swift`: phase switch unchanged; `.ready` (no first-device, no trust pending) → `MainSplitView`; gating screens fill the window without a sidebar; min 620x420.
- `Shell/MainSplitView.swift`, `SidebarView.swift`, `MainToolbar.swift`:
  - Sidebar rows Devices / History / Transfers + offer-count badge; bottom "Pair device…" button.
  - Detail → phase 03 pages; `.sheet` PairingSheet.
  - Toolbar: static status pill + Pair device….
- Menubar "History…" / "Add device…" route via `navigation`; a "Transfers…" entry appears when an offer or active transfer exists.
- Popover restyle (`MenuPopover`, `Menu/*`): sections per spec. Width 280; `MenuWindowTopAnchor` kept. The existing drop-to-send stays as today.
- Settings (`Settings/*`): TabView General, Sync, Privacy, Storage, Account; grouped `Form`s; save-on-change unchanged; Account tab = email, node prefix, Sign out (confirmed).
- Delete `ReadyView.swift` and the history window code.

## Architecture
```
FsyncMacApp
 ├ MenuBarExtra ── MenuPopover(bridge, actions → navigation + showMain/showSettings)
 ├ Window "settings-opener" (alive, invisible) ── SettingsOpenerView: SettingsRequest → openSettings()
 ├ Window "main" ── RootView ── gating | MainSplitView(sidebar(selection: MainSection?), detail, pairing sheet)
 └ Settings ── SettingsView(TabView of grouped Forms)
showMain(): openWindow("main") → activate → NSApp.windows.first { identifier == fsync.main }.makeKeyAndOrderFront
```

## Related Code Files
- Modify: `Sources/FsyncMac/FsyncMacApp.swift`, `Sources/FsyncMacUI/{RootView,MenuPopover,TrustGate,SignOutButton}.swift`, `Menu/{MenuBarContentView,StatusHeaderView,ClipboardMenuSection,FilesMenuSection,MenuWindowTopAnchor}.swift`, `Settings/{SettingsView,SyncSettingsView,PrivacySettingsView,StorageSettingsView}.swift`, `Tests/FsyncMacUITests/MenuWindowTopAnchorTests.swift` (only if the anchor API changes)
- Create: `Sources/FsyncMac/SettingsOpener.swift`, `Sources/FsyncMacUI/Shell/{MainSplitView,SidebarView,MainToolbar,WindowIdentifier}.swift`, `Settings/{GeneralSettingsView,AccountSettingsView}.swift`, `Tests/FsyncMacUITests/ShellRenderTests.swift`
- Delete: `Sources/FsyncMacUI/ReadyView.swift`
- Removed vs old plan: `Shell/GlassChrome.swift`, `FsyncCommands.swift`, `ActivationPolicyController.swift`, `IncomingFilesNotifier.swift` (moved to 05/06 or deleted). <!-- red-team #6 -->
(paths relative to `apps/macos/FsyncMac/`)

## Implementation Steps
0. Spike: `SettingsOpener` + `WindowIdentifier` + opener scene only; run the pass criteria on macOS 26; write the spike report. Continue only on pass. <!-- red-team #4 -->
1. `MainSplitView` + `SidebarView` + `MainToolbar`; `RootView` ready branch; pairing sheet; delete `ReadyView`.
2. Scenes: remove the history window; route menubar actions via `navigation`; `showMain` by identifier.
3. Replace `showSettings()` sendAction with the opener request.
4. Settings restyle; 5. Popover restyle.
6. `ShellRenderTests`: MainSplitView offscreen per section (sidebar row count 3; `nil` selection renders Devices); RootView per `AppPhase`; SettingsView each tab.
7. `swift build && swift test`; `swift run FsyncMac` smoke.

## Todo List
- [ ] Spike openSettings on macOS 26 (report; gate)
- [ ] Split view + sidebar + toolbar + pairing sheet
- [ ] RootView gating, ReadyView deleted
- [ ] History window removed, menubar routes via navigation, showMain by identifier
- [ ] Settings via opener; grouped forms
- [ ] Popover restyle
- [ ] ShellRenderTests

## Success Criteria
- Spike report exists with PASS on macOS 26; macOS 14 listed as untested risk.
- `cd apps/macos/FsyncMac && swift build && swift test` green.
- `grep -rn 'id: "history"\|title == "fsync"\|showSettingsWindow\|glassEffect' apps/macos/FsyncMac/Sources/FsyncMac/FsyncMacApp.swift apps/macos/FsyncMac/Sources/FsyncMacUI` is empty; `ReadyView.swift` is gone.
- `swift run FsyncMac` on macOS 26: popover opens; "History…" opens the main window on History with fresh rows; ⌘, and popover "Settings…" open Settings in front.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| openSettings workaround fails on 14 or 26 | M x H | Blocking spike first; stop + user decision; no sendAction fallback claim |
| Opener window visible/focusable | M x M | alpha 0, ignoresMouseEvents, borderless, excluded from the Window menu and cycling; spike checks it |
| No macOS 14 host | M x H | Accepted by user (Validation 1): 14 untested, noted in QA checklist + release note |
| Identifier not set before first lookup | L x M | Fall back to `openWindow` + next runloop retry once |

## Security Considerations
- Sign out stays a confirmed destructive action (Cancel default).

## Next Steps
- Phase 05 (notifications) owns AppDelegate and reuses `WindowIDs`. Rollback: revert phase commits; phases 02-03 stay valid (pages unused; the old shell returns).
