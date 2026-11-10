# Brainstorm: Linux native app

Date: 2026-11-06. Status: approved by the user.

## Problem
Add a Linux desktop app equal to the macOS/Windows apps. Targets: Omarchy/Arch, Ubuntu, Debian, Fedora. Build and test on the local Omarchy machine (Hyprland, Wayland, Qt 6.12). Cross-OS sync test against Windows on the Rig worker `waso-vm`.

Already there: `fsync-core` and `fsync-cli` build and run on Linux (the netns matrix runs on Linux). Missing: the desktop app (tray, sign-in, pairing, clipboard, files, history, settings).

## Requirements (agreed)
| Item | Decision |
|---|---|
| UI | Qt6/QML (user choice) |
| Core bridge | Rust + CXX-Qt (KDAB): QObjects in Rust, exposed to QML, call `FsyncService` directly |
| Min distro | Ubuntu 22.04 / Debian 12 (glibc 2.35, Qt 6.2) |
| Arch | x86_64 only |
| GNOME Wayland | auto sync where data-control exists (wlroots, Hyprland, KDE) and on X11; manual send on GNOME. No GNOME Shell extension in v1 |
| File paste | eager download to inbox, `text/uri-list` on the clipboard; above 20 MiB: notification + open folder |
| Packaging | AUR PKGBUILD, .deb, .rpm, AppImage. No Flatpak (sandbox blocks clipboard watch and shortcuts) |
| Denylist | accepted limit: by app name only on X11; Wayland uses MIME hints (`x-kde-passwordManagerHint=secret`) |

## Approaches evaluated
| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Rust + Slint / iced | fewest runtime deps, simplest packaging | not the user's choice; Slint needs attribution | rejected by the user |
| Rust + GTK4/libadwaita | native on GNOME | must target GTK 4.6 / adw 1.1; AppImage with GTK is fragile | rejected |
| C++ Qt + uniffi-bindgen-cpp | standard UniFFI path | generator supports uniffi 0.28.3 only; repo pins `=0.29.4` (ADR 008) | not viable |
| C++ Qt + hand-written C ABI | no generator coupling | second binding layer to maintain | rejected (DRY) |
| **Rust + CXX-Qt + QML** | QML UI, no extra FFI layer, cargo build, one language for logic | CXX-Qt has no stated Qt 6.2 test matrix | **chosen** |

## Design
- `apps/linux/`: separate Cargo workspace (path deps to `core/`), so `cargo test --workspace` on macOS/CI does not need Qt.
- Rust modules: `engine-bridge` (QObject over `FsyncService`, callbacks to the Qt thread), `clipboard` (backend chosen at runtime: `ext-data-control-v1` / `zwlr-data-control-v1`, X11 XFixes, manual), `secrets` (Secret Service over D-Bus), `hotkeys` (portal GlobalShortcuts plus `fsync --clip-send` / `--clip-paste` for compositor binds), single instance per data dir (core `engine.lock`) with `--send <paths>` forwarded to the running instance.
- QML: tray (`QSystemTrayIcon`), sign-in, pairing (code/QR/SAS), devices, history, settings, and a main window, because Fedora GNOME has no tray without the AppIndicator extension.
- Features match Windows phases 13+14: text/html/png both ways, auto/manual, eager files, history copy-back, retention, drag-and-drop send. File-manager "Send to" is deferred.
- Autostart: XDG autostart `.desktop`.

## Build, packaging, tests
- Dev builds on Omarchy. Release builds in an Ubuntu 22.04 container (glibc 2.35, Qt 6.2).
- Packages: PKGBUILD (Arch: qt6-base, qt6-declarative), .deb/.rpm against distro Qt, AppImage via linuxdeploy-plugin-qt (~70-90 MB).
- Tests: Rust unit tests; Linux app <-> `fsync-cli` e2e locally; install smoke in Ubuntu 22.04 / Debian 12 / Fedora containers; cross-OS: Linux app here <-> `fsync-cli`, then the Windows app on `waso-vm` via `rig remote run` (the GUI app needs `taskMode: interactive`).

## Phases
1. CXX-Qt scaffold, engine bridge, tray, sign-in/pairing/devices, ADR 014.
2. Clipboard backends, eager files, history, settings, hotkeys.
3. Packaging (PKGBUILD/deb/rpm/AppImage) and distro install smoke tests.
4. Cross-OS test with waso-vm.

## Risks
| Risk | Mitigation |
|---|---|
| QML/API used on Qt 6.12 is missing in 6.2 | CI build + QML smoke in an Ubuntu 22.04 container from phase 1 |
| CXX-Qt on Qt 6.2 is unverified | phase 1 spike: build hello QObject in the 22.04 container first; fallback: raise the floor to Ubuntu 24.04 (Qt 6.4) |
| GNOME users get manual send only | documented; GNOME extension can come later |
| Wayland gives no source app, so the denylist is weak | MIME hints; documented in ADR 014 and in Settings |
| Secret Service missing (minimal WMs) | fall back to a 0600 file with a visible warning |
| waso-vm Rig `rig-version` FAIL (same 1.0.0, scripts differ) | user reinstalls Rig on waso-vm, then `remote setup waso-vm` |

## Success criteria
- App installs from each package on Omarchy, Ubuntu 22.04, Debian 12, Fedora; starts, signs in, pairs.
- Clip text/image Linux <-> Windows (waso-vm) both ways in ≤ 5 s; file round trip with matching hash.
- Auto sync on Hyprland; manual send works on GNOME Wayland.

## Unresolved questions
- Exact Fedora version to support (latest only?).
- Is the Slint/iced trade-off final, or open again if CXX-Qt on Qt 6.2 fails?
- Can the GUI on waso-vm run in an interactive Rig task, or does the user drive it by hand?
