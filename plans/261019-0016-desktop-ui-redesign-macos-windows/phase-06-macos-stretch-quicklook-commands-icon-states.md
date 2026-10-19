---
owns: ["apps/macos/FsyncMac/Sources/FsyncMacCore/Features/Navigation/MenuBarIconState.swift", "apps/macos/FsyncMac/Tests/FsyncMacCoreTests/MenuBarIconStateTests.swift", "apps/macos/FsyncMac/Sources/FsyncMac/FsyncCommands.swift", "apps/macos/FsyncMac/Sources/FsyncMac/ActivationPolicyController.swift"]
dependsOn: [05]
---

# Phase 06 (OPTIONAL stretch): Quick Look, symbolEffect, commands, menubar icon states (this Mac)
<!-- red-team #6: stretch items moved out of MVP phases 02-04 -->

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` (icon map "Stretch" rows)
- Research: `research/researcher-261019-macos-native-ui-patterns.md` (quickLookPreview, symbolEffect, CommandMenu in accessory apps)
- MVP outputs: phases 02-05

## Overview
- Priority: P3. Status: pending. Optional: the redesign ships without it. Each item is independently droppable. Run only after phase 05 is Completed.
- Items:
  - (a) Quick Look on finished transfer rows and history file rows;
  - (b) `symbolEffect` on the menubar label and active transfer progress;
  - (c) `CommandMenu("Go")` ⌘1/⌘2/⌘3, ⌘N + `ActivationPolicyController`. Dock icon approved (Validation 1), so (c) is in the stretch; <!-- Updated: Validation Session 1 - Dock icon approved; handoff exception approved -->
  - (d) menubar icon state variants.

## Key Insights
- Handoff rule (local only; Swift has no partial types, so a stretch feature cannot attach without editing its host file): this phase owns only the new files in frontmatter. It may make additive edits to exactly these MVP files, after their owner phase is Completed and committed, sequentially on this Mac (never in parallel). The MVP phase stays the owner of record. Rollback = revert this phase's commits.
  - `Sources/FsyncMacCore/Bridge/EngineBridge.swift` (02): one stored `@Published menuBarIconState`
  - `Sources/FsyncMacCore/Bridge/EngineBridge+Models.swift` (02): Combine sinks
  - `Sources/FsyncMacUI/Transfers/TransfersTableView.swift`, `Sources/FsyncMacUI/History/HistoryRowView.swift` (03): Quick Look, symbolEffect
  - `Sources/FsyncMac/FsyncMacApp.swift` (04): `.commands`, label state + symbolEffect
  - `Sources/FsyncMac/AppDelegate.swift` (05): ActivationPolicyController hookup
- (d) red-team #8 (modified) <!-- red-team #8 -->: the bridge gets a stored `@Published private(set) var menuBarIconState: MenuBarIconState`. Combine sinks on `transfers.$rows`, the trust-prompt pending publisher and `$phase` (+ `$anyPeerOnline`) recompute it via the pure `MenuBarIconState.compute(phase:anyPeerOnline:activeTransfers:trustPending:)`, with attention winning (bootFailed or trust pending). A computed property in the label was rejected because SwiftUI would not observe nested models.
- Quick Look only for existing file URLs (`FileManager.fileExists`); text/image clips get none.
- `CommandMenu` items are visible only while the policy is `.regular`. The controller sets `.regular` while the main or Settings window is visible and `.accessory` when both close, debounced one runloop.

## Requirements
- (a) `.quickLookPreview($previewURL)` + row action "Quick Look" (Space when the row is selected) in the two row files.
- (b) `.symbolEffect(.pulse, isActive:)` on active progress; `.symbolEffect(.bounce, value: menuBarIconState)` on the label.
- (c) `FsyncCommands` (Go: Devices ⌘1, History ⌘2, Transfers ⌘3; Pair device… ⌘N); `ActivationPolicyController`. Omit any shortcut already bound by `HotkeyManager`.
- (d) `MenuBarIconState` + stored bridge property + sinks; the label shows `idle | online | transferring | attention` symbols from the spec.
- Success criteria are scoped to the items actually shipped; the phase report lists dropped items.

## Architecture
```
transfers.$rows ─┐
trust pending  ──┼─ CombineLatest → MenuBarIconState.compute → bridge.menuBarIconState (@Published) → MenuBarExtra label
$phase/$anyPeerOnline ┘
Window visibility (main/settings) → ActivationPolicyController → NSApp.setActivationPolicy (.regular/.accessory)
```

## Related Code Files
- Create: `Sources/FsyncMacCore/Features/Navigation/MenuBarIconState.swift`, `Tests/FsyncMacCoreTests/MenuBarIconStateTests.swift`, `Sources/FsyncMac/FsyncCommands.swift`, `Sources/FsyncMac/ActivationPolicyController.swift`
- Modify (handoff, additive; exception approved by user, Validation 1): the 6 files listed in Key Insights
- Delete: none
(paths relative to `apps/macos/FsyncMac/`)

## Implementation Steps
1. (d) `MenuBarIconState` table test; bridge stored property + sinks; bridge-level test: a fake transfer event → `.transferring`; a trust pending → `.attention`; boot failed → `.attention`; cleared → `.idle`/`.online`.
2. (b) symbolEffects. 3. (a) Quick Look.
4. (c): commands + policy controller; manual check that the Dock icon disappears after closing all windows.
5. `swift build && swift test`.

## Todo List
- [ ] (d) MenuBarIconState + @Published bridge state + sinks + bridge test
- [ ] (b) symbolEffect
- [ ] (a) Quick Look
- [ ] (c) CommandMenu + ActivationPolicyController (Dock icon approved)
- [ ] Report: shipped vs dropped items

## Success Criteria
- `cd apps/macos/FsyncMac && swift build && swift test` green, incl. `MenuBarIconStateTests` (table + bridge-level) if (d) shipped.
- (a): Quick Look opens for a done transfer on disk; no menu item when the file is missing.
- (c): ⌘1-3 switch sections; the Dock icon is gone after the last window closes (macOS 26).
- `git diff` of this phase touches only the frontmatter files + the 6 handoff files.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Policy flip flickers / steals focus | M x M | Flip only on visibility change; debounce; Dock icon approved |
| Sinks retain cycles | L x M | `[weak self]`, cancellables in the bridge |
| Handoff edit conflicts with MVP fixes | L x L | Sequential only; rebase on the latest feat branch first |

## Security Considerations
- Quick Look only on engine-reported paths that exist. No new data surfaces.

## Next Steps
- Phase 12 QA rows for shipped items only. Rollback: revert phase commits; MVP unaffected.
