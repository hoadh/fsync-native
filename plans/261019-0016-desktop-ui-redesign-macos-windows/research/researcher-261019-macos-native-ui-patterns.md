# Research: macOS-native UI for fsync redesign (2026-10-19)

Confidence medium: HIG pages not readable; availability versions mostly from memory.

## Constraint
- `apps/macos/FsyncMac/Package.swift`: `.macOS(.v13)`. User decision 2026-10-19: bump to `.v14`. macOS 26 Liquid Glass behind `#available(macOS 26, *)`.

## Layout
- Main `Window` (single instance, not `WindowGroup`) with `NavigationSplitView`: Devices, History, Transfers, Pair.
- macOS 26 SDK: sidebar floats as Liquid Glass automatically; glass on chrome only, not content (WWDC25 #323).
- Toolbar: few grouped items (Pair device, sync status).
- `.searchable` default placement on History + Transfers.
- `ContentUnavailableView` (14+) empty states. `.inspector` (14+) only if needed (YAGNI).
- `Table` for Transfers (name, size, progress, direction, status); `List` with 2-line rows for History.
- Symbols: `desktopcomputer`/`laptopcomputer`/`pc` devices, `doc.on.clipboard` history, `arrow.up.arrow.down.circle` transfers, `qrcode.viewfinder` pairing.

## SwiftUI pitfalls
- Settings from accessory (LSUIElement) app: `openSettings` (14+) often fails to bring window forward; reported broken on Tahoe without workaround. Workaround: hidden 1x1 `Window` declared before `Settings` reading `@Environment(\.openSettings)`; switch activation policy `.regular`, activate, `openSettings()`, front window; back to `.accessory` on close (steipete 2025, single source). Must test 14 + 26.
- `defaultSize`, `windowResizability` (13+). `.defaultLaunchBehavior(.suppressed)`, `.restorationBehavior` (15+) — today AppDelegate closes main window at launch (13 workaround), keep that on 14.
- `MenuBarExtra(.window)`: no reliable programmatic open/close without helper (MenuBarExtraAccess); drop onto popover untested. Prototype before relying.
- Boot/sign-in = gating states replacing main window content. Pairing = `.sheet` from Pair item / toolbar, QR inline.
- `.dropDestination(for: URL.self)` on device row to send files.
- QR: CoreImage `CIQRCodeGenerator` (exists: `QrCodeImage`).
- `.confirmationDialog` for unpair / clear history.

## Settings
- `TabView` of grouped `Form` (`.formStyle(.grouped)`) in `Settings` scene; 3-5 panes. Sidebar settings only if >6 panes.

## Delight (value/cost)
1. `.symbolEffect` (14+) on sync status, transfer progress — near zero cost.
2. Keyboard shortcuts + `CommandMenu` — low cost.
3. `UNUserNotificationCenter` for incoming files — needs bundle id + signing; incoming files only.
4. Quick Look (`.quickLookPreview`) on history/transfer items.
5. Menubar icon state: swap symbol variants; animation limited.

## Testing (SwiftPM)
- `ImageRenderer` smoke (existing). Blank/wrong for AppKit-backed `List`/`Table`/`NavigationSplitView`.
- `NSHostingView` in offscreen `NSWindow` for sidebar/table layouts.
- swift-snapshot-testing only if needed.

## Sources
- https://developer.apple.com/videos/play/wwdc2025/323/
- https://steipete.me/posts/2025/showing-settings-from-macos-menu-bar-items/

## Unresolved
- MenuBarExtra(.window) drops + programmatic close on 26: prototype.
- openSettings workaround on 26: verify.
