# macOS openSettings spike (phase 04), 2026-10-19

Host: macOS 26 (this Mac). macOS 14 untested (risk accepted, Validation Log).

- Setup: scratch SwiftPM app outside repo, copies of `SettingsOpener.swift` + `WindowIdentifier.swift`; real mouse clicks + AX window checks.
- Popover → Settings, no window open: Settings front + key 5/5, incl. close/reopen.
- Main window open: popover → Settings and ⌘, both bring Settings front + key.
- Opener window: never key, not in Window menu, offscreen (-10000, 11116), alpha 0.
- `.regular` activation policy NOT needed: same 5/5 with `.accessory`. Final code leaves policy alone (Dock icon never shown).
- SwiftUI resets the opener's `NSWindow.identifier` to its scene id after restyle → `WindowIDs.isSettingsOpener` accepts both. Main window keeps `fsync.main`.

Unresolved: behaviour on macOS 14.
