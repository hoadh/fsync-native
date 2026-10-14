# Phase 05: Settings window, launch at login, FsyncMac.app bundle script

## Context Links
- Spec phase-11 requirements (SMAppService.mainApp, hardened runtime)
- scripts/build-xcframework.sh (FsyncFFI must exist before building)

## Overview
Priority P2 · Status completed (manual bundle checks open) · 4h.
- A basic Settings scene with launch at login, account (email/account id, Sign out) and an "About" line with the node id prefix.
- A script that builds the release binary and assembles a signed `.app`.

## Key Insights
- `SMAppService.mainApp` needs a real bundle. Under `swift run` the status stays `.notFound`, so the UI disables the toggle and explains why.
- `.requiresApproval` must deep-link to System Settings (`SMAppService.openSystemSettingsLoginItems()`).
- An ad-hoc signature changes its identity on every build. Launch-at-login registration and Keychain ACLs may need re-approval after rebuilds, which is acceptable for dev.
- `swift build -c release --product FsyncMac` produces a plain Mach-O. Bundle it as `Contents/MacOS/FsyncMac`.

## Requirements
- LaunchAtLogin (FsyncMacCore): `status`, `setEnabled(Bool) throws`; errors are shown inline.
- SettingsView tabs: General (launch at login), Account (Sign out with a confirm, which keeps the device keys and history).
- Script `apps/macos/scripts/build-fsync-mac-app.sh`:
  1. `--api URL --relay URL --bundle-id ID --out DIR`. Defaults: `http://127.0.0.1:8080`, none, `app.fsync.mac`, `apps/macos/build/`.
  2. Checks that `FsyncFFI.xcframework` exists and otherwise tells the user to run `scripts/build-xcframework.sh`. It does not call it.
  3. `swift build -c release --product FsyncMac --package-path apps/macos/FsyncMac`.
  4. Assembles `FsyncMac.app/Contents/{MacOS/FsyncMac, Info.plist, Resources/}`.
  5. Info.plist: `CFBundleIdentifier`, `CFBundleName`, `CFBundleExecutable`, `CFBundlePackageType APPL`, `CFBundleShortVersionString`, `LSMinimumSystemVersion 13.0`, `LSUIElement true`, `FsyncApiBase`, `FsyncRelayUrl` (written with `plutil`).
  6. `codesign --force --options runtime --timestamp=none -s - FsyncMac.app`, then `codesign --verify --strict`.
  7. Prints the path. `set -euo pipefail`; the script never touches `scripts/**`.
- Add `apps/macos/build/` to `.gitignore` (under apps/macos).

## Architecture
```
SettingsView → LaunchAtLogin (SMAppService.mainApp) ; Sign out → EngineBridge.signOut()
build-fsync-mac-app.sh → swift build (release) → bundle layout → plutil → codesign (ad-hoc, hardened)
```

## Related Code Files
Create:
- `Sources/FsyncMacCore/Settings/LaunchAtLogin.swift`
- `Sources/FsyncMacUI/Settings/SettingsView.swift`
- `apps/macos/scripts/build-fsync-mac-app.sh` (kebab-case, executable)
- `apps/macos/.gitignore` (`build/`)
- `Tests/FsyncMacCoreTests/LaunchAtLoginTests.swift`: status readable, no crash outside a bundle

Modify:
- `FsyncMacApp.swift`: `Settings { SettingsView }`; menu action "Settings…" (`SettingsLink` on 14+, `NSApp.sendAction(showSettingsWindow:)` on 13)

## FFI calls
`logout()` (through `EngineBridge.signOut`), `nodeId()`.

## Implementation Steps
1. LaunchAtLogin wrapper and test.
2. SettingsView. Sign out uses a confirm and then `signOut`.
3. Bundle script, run locally, then `codesign -dv --verbose=2` shows `flags=0x10000(runtime)` and `Signature=adhoc`.
4. Manual checks with the bundle:
   - Launch: the icon appears and there is no Dock icon.
   - Sign-in works and the Keychain items exist (`security find-generic-password -s app.fsync.mac`).
   - The launch-at-login toggle registers and survives a logout/login.

## Todo List
- [x] LaunchAtLogin + test (`SettingsTests`; also `PairingPasteboard` + test, "Settings…" in popover)
- [x] SettingsView (General, Account); `Settings` scene opened via responder-chain action
- [x] build-fsync-mac-app.sh (codesign adhoc+runtime, plutil lint, LSUIElement, open/quit smoke OK)
- [ ] Manual bundle checks recorded (done: launch as UIElement, quit. Not done: sign-in from bundle, launch-at-login toggle, Settings window visual)

## Success Criteria
- The script exits 0 on a clean checkout that has the xcframework. `codesign --verify --strict` passes, and `plutil -lint` passes on the Info.plist.
- The app runs as an agent (no Dock icon). The toggle reflects the `SMAppService` status.

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| SMAppService refuses ad-hoc app | L×M | Show the status text; the user can approve in System Settings. Note it for Developer ID. |
| Hardened runtime blocks loading the static lib | L×L | Rust is linked statically into the binary; no dylibs to validate |
| Release build differs from debug (optimiser and concurrency) | L×M | Phase 6 runs the integration against debug; a manual smoke uses the release bundle |

## Security Considerations
- Hardened runtime from day 1 with no entitlements, so there are no exceptions such as `disable-library-validation`.
- Sign out clears the tokens from the Keychain and the core (`logout` also drops presence and the pinned trust).

## Next Steps
Phase 6 is the integration run.
