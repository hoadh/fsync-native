# Research: CXX-Qt, Linux tray, Qt runtime pkgs, packaging

Scope: Rust + CXX-Qt + QML desktop app, Rust core (tokio, SQLCipher + OpenSSL vendored). Targets: Ubuntu 22.04 (Qt 6.2.4), Debian 12 (Qt 6.4.2), x86_64. Dev: Arch, Qt 6.12, Hyprland.
Budget: 8 web lookups. Several gaps remain (see Unresolved). Items marked **[unverified]** come from memory, not a fetched source.

## 1. CXX-Qt

| Item | Finding | Source |
|---|---|---|
| Latest release | 0.10.0 ("Latest", dated 24 Aug; page omits year). Prior: 0.9.1 (03 Jul), 0.9.0 (23 Jun). Year not confirmed, assumed 2026. | https://github.com/KDAB/cxx-qt/releases |
| 0.10.0 notes | Only "Added": QJsonArray/QJsonObject/QJsonValue, ItemFlag, ItemDataRole. No breaking changes listed. | same |
| Min Qt 6 version | **Not found.** Book page 404'd; release page silent. | - |
| Qt 6.2 build | **Unknown.** Must test: `ubuntu:22.04` container, `cargo build` against apt Qt 6.2.4. Do not assume. | - |
| CI platforms | Linux, Windows, macOS, all x86_64. wasm32 compiled, not CI-tested. | https://kdab.github.io/cxx-qt/book/ |
| Qt discovery | Env `QMAKE` (path to qmake6/qmake) is the usual mechanism **[unverified in this session]**. Verify by grepping cxx-qt-build source. | - |
| QML registration | `QmlModule` builder in `build.rs` (uri, qml_files, rust QObjects) **[unverified syntax]**. Check the book's "first project" chapter and cxx-qt-build docs on docs.rs. | - |
| Thread to Qt | `qt_thread()` on the Rust QObject wrapper returns a `CxxQtThread<T>` (Send, cloneable). Worker calls `.queue(\|qo\| { ... })`; emit signals inside the closure, never from the worker thread. **[unverified exact API]** | - |

Risk: CXX-Qt is KDAB-maintained, pre-1.0 (0.x), with frequent minor releases. Pin the exact version in `Cargo.lock`. Each minor can change the bridge macro API.

Architectural fit:
- Rust core stays sync-API. Call it from tokio worker threads (or `spawn_blocking`), never on the Qt GUI thread (SQLCipher I/O would freeze UI).
- Callback trait impl holds a `CxxQtThread` clone, queues the signal emit. Keeps Qt types out of the core.
- Ubuntu 22.04 Qt 6.2 is the lowest common denominator. If CXX-Qt needs ≥6.4 or 6.5, options: (a) bump min target to Debian 12 / Qt 6.4, (b) ship AppImage with Qt installed via aqtinstall/official binaries built in the 22.04 container.

## 2. System tray

| Option | Needs | Linux behavior | Verdict |
|---|---|---|---|
| `QSystemTrayIcon` (Widgets) | `QApplication` (not `QGuiApplication`) | Uses SNI over D-Bus on KDE/GNOME-with-ext/Xfce/LXQt. Docs also list XEmbed for X11-only WMs. `isSystemTrayAvailable()` for checks. Balloon messages unreliable. | Avoid: forces Widgets into a QML app. Source: https://doc.qt.io/qt-6.4/qsystemtrayicon.html , https://doc.qt.io/qt-6.2/qsystemtrayicon.html |
| `Qt.labs.platform SystemTrayIcon` (QML) | `QGuiApplication` + qml6 labs module | Delegates to QPA platform theme → same SNI path. | Viable. Labs = no stability promise. Package name on Ubuntu 22.04 **[unverified]**: `qml6-module-qt-labs-platform`. |
| Rust SNI crate (e.g. `ksni`, zbus-based) **[unverified crate state]** | zbus/D-Bus only | Talks SNI directly; same code path on every desktop. Independent of Qt version. | **Preferred** if tray needs are simple (icon, menu, activate). |

SNI by desktop:
- **Hyprland (bare compositor):** no StatusNotifierWatcher by default. The bar must host one. Waybar's tray module does this. Startup races are known: icons missing when apps launch before waybar starts; reported on Gentoo/OpenRC when Hyprland not started under `dbus-run-session`. Sources: https://docs.rs/crate/tablero/latest/source/README.md , https://forums.gentoo.org/viewtopic.php?p=8823915
- **KDE Plasma:** native SNI. Works.
- **GNOME:** no native SNI. Needs the "AppIndicator and KStatusNotifierItem Support" shell extension **[unverified in this session, widely known]**. Without it, tray icon is invisible. Fallback: app must still work with window open.

Recommendation (ranked):
1. Rust SNI crate (ksni or equivalent). One path, no Qt dependency for tray, tray works on Hyprland/KDE, document GNOME extension requirement.
2. Qt.labs.platform SystemTrayIcon, if team prefers Qt-only. Accept labs API risk.
3. QSystemTrayIcon. Reject unless app already uses Widgets.

Always: app must be usable without a tray (GNOME, or Waybar not running). Tray is a convenience, not the only entry point.

## 3. Runtime package names

Qt 6.2 on Ubuntu 22.04 (qt6-declarative 6.2.4+dfsg-3ubuntu1 confirmed for controls):

| Need | Ubuntu 22.04 (Qt 6.2) | Debian 12 (Qt 6.4) | Fedora 40+ | Arch |
|---|---|---|---|---|
| QML core | `qml6-module-qtqml-*`, `qml6-module-qtquick` **[unverified name]** | `qml6-module-qtquick` **[unverified]** | `qt6-qtdeclarative` | `qt6-declarative` |
| Controls | `qml6-module-qtquick-controls` (confirmed, 6.2.4) | `qml6-module-qtquick-controls` **[unverified]** | `qt6-qtdeclarative` | `qt6-declarative` |
| Layouts | `qml6-module-qtquick-layouts` **[unverified]** | same **[unverified]** | `qt6-qtdeclarative` | `qt6-declarative` |
| Window | `qml6-module-qtquick-window` **[unverified]** | same **[unverified]** | `qt6-qtdeclarative` | `qt6-declarative` |
| Labs platform (tray) | `qml6-module-qt-labs-platform` **[unverified]** | same **[unverified]** | `qt6-qtdeclarative` | `qt6-declarative` |
| Wayland platform | `qt6-wayland` **[unverified]** | `qt6-wayland` **[unverified]** | `qt6-qtwayland` | `qt6-wayland` |

Source: https://packages.ubuntu.com/ru/jammy/qml6-module-qtquick-controls (Ubuntu 22.04: 6.2.4, universe, ~935 kB download). Noble ships 6.4.2 → different minor. Verify all **[unverified]** names with `apt-file search` or packages.ubuntu.com before writing deps. Action: run `apt-cache policy` and `ldd` on the built binary in the container; let `ldd` + `qmlimportscanner` decide the list.

Arch: `qt6-declarative` covers all QML modules in one package. Arch-built binary (Qt 6.12) **will not run** on 6.2/6.4 hosts. Qt minor-version ABI: never ship a binary built on dev machine.

## 4. Packaging

Ranked for this app:

1. **AppImage (primary, distro-agnostic).** Bundles Qt, QML imports, Wayland plugin. Build in `ubuntu:22.04` (lowest glibc baseline). Use linuxdeploy + linuxdeploy-plugin-qt.
   - Plugin env (verified): `QML_SOURCES_PATHS` (dir with QML; plugin scans imports), `QML_MODULES_PATHS` (extra, rarely needed), `EXTRA_QT_MODULES` (e.g. `waylandcompositor`), `EXTRA_PLATFORM_PLUGINS` (e.g. `libqwayland-egl.so;libqwayland-generic.so`), `QMAKE` (pick correct Qt), `DEBUG`.
   - Wayland is NOT bundled by default. Add `EXTRA_PLATFORM_PLUGINS` explicitly. Otherwise app falls back to xcb (XWayland) on Hyprland.
   - Plugin supports Qt 5 and Qt 6. No known-issues section on the page; test.
   - Size: estimate 60–120 MB (Qt Quick + QML + Wayland + SQLCipher-free). Measure, do not trust estimate.
   - Source: https://github.com/linuxdeploy/linuxdeploy-plugin-qt
2. **.deb per target (Ubuntu 22.04 / Debian 12).** cargo-deb: `[package.metadata.deb]` with `depends` (auto via `$auto`, or explicit). Assets as triples. Build each target in its own container. Limits: Qt deps differ by distro, so one .deb cannot cover both unless deps are generic (`libqt6core6 | ...`). Source: https://github.com/kornelski/cargo-deb
3. **.rpm (Fedora 40+).** cargo-generate-rpm: no rpmbuild needed; deps under `[package.metadata.generate-rpm.requires]`; `auto-req = "disabled"` to avoid wrong autodetection. Source: https://github.com/juliusl/cargo-generate-rpm
4. **PKGBUILD (Arch).** Build from source with `makepkg`, `depends=('qt6-declarative' 'qt6-wayland')`, `makedepends=('cargo' 'qt6-tools')`. Arch-only, low priority; dev convenience.

Tradeoffs: AppImage = largest, no install, no distro deps, works everywhere with glibc ≥2.35. .deb/.rpm = small, but a matrix to maintain. Choose AppImage first; add .deb if users want native install.

## 5. Pitfalls

- **Qt 6.2 vs 6.12 QML:** Qt Quick Controls changed a lot. Basic style exists in 6.2 but lacks newer controls and some properties. Use `QT_QUICK_CONTROLS_STYLE=Basic` or `qtquickcontrols2.conf` only with tested controls. Avoid `Qt.labs.*` beyond tray. Avoid Material/Universal newer features. Test the UI on 6.2 before release, not only on Arch.
- **Qt ABI:** Build release only in the oldest target container. Binary from newer Qt won't load on older Qt.
- **Rust toolchain in Ubuntu 22.04:** apt rustc is old (1.75 in jammy). Use `rustup` inside the container and pin `rust-toolchain.toml`. Check CXX-Qt MSRV. **[unverified version]**
- **Vendored crypto builds:** OpenSSL vendored needs `perl` + `make`. SQLCipher vendored needs C compiler (`build-essential`), often `tcl` only for upstream tests (not needed). Install `pkg-config`, `libclang-dev` if bindgen is used (CXX-Qt generated code may not need it; verify).
- **tokio + Qt:** Do not `block_on` in GUI thread. Use `tokio::runtime::Runtime` owned by a Rust singleton, spawn sync core calls via `spawn_blocking`.
- **Wayland fractional scaling / Hyprland:** Qt 6.2 Wayland support is weaker than 6.4+. Expect bugs on Hyprland with 6.2. Need explicit test.
- **Tray startup race:** tray icon registered before bar hosts watcher → invisible. Retry registration or check watcher on demand (QSystemTrayIcon has no retry; ksni/zbus implementation needed).
- **Container build without GPU/display:** Qt tests need `QT_QPA_PLATFORM=offscreen`. Headless CI can compile and run `qmlimportscanner` only.

## Recommendation (ranked)

1. **Build release in `ubuntu:22.04` container with rustup + apt Qt 6.2.4.** First action: confirm CXX-Qt pinned version compiles against Qt 6.2. If not, drop min target to Debian 12 (Qt 6.4) or switch Qt source to aqtinstall official Qt for AppImage.
2. **Ship AppImage** (linuxdeploy + linuxdeploy-plugin-qt, Wayland plugins explicit). Secondary: `.deb` for Ubuntu 22.04 / Debian 12 only.
3. **Tray via Rust SNI crate**, not QSystemTrayIcon. Qt.labs.platform fallback. GNOME requires extension, documented.
4. **Runtime deps:** determine from `ldd` + `qmlimportscanner` in the container, not from memory.
5. **Arch PKGBUILD** only for dev convenience; never release from dev machine.

## Unresolved questions

- Exact minimum Qt for CXX-Qt 0.10.0. Need `cxx-qt-build/Cargo.toml` or README on crates.io/docs.rs.
- Exact CXX-Qt build-script API (QmlModule, qt_thread/queue signature) and version-matched example; book pages 404'd.
- Does CXX-Qt 0.10.0 compile on Qt 6.2.4? (Only empirical test resolves.)
- Exact Ubuntu 22.04 / Debian 12 binary package names for QML modules and Wayland plugin (only controls confirmed).
- Current ksni (or equivalent) maturity, maintenance, and Hyprland/GNOME behavior.
- Real AppImage size for this app (estimate only).
- Whether Hyprland/Waybar tray needs any extra config beyond default `tray` module.
- Release year of CXX-Qt 0.10.0 (page shows "24 Aug" only).

## Sources

- https://github.com/KDAB/cxx-qt/releases
- https://kdab.github.io/cxx-qt/book/
- https://doc.qt.io/qt-6.2/qsystemtrayicon.html
- https://doc.qt.io/qt-6.4/qsystemtrayicon.html
- https://packages.ubuntu.com/ru/jammy/qml6-module-qtquick-controls
- https://github.com/linuxdeploy/linuxdeploy-plugin-qt
- https://docs.rs/crate/tablero/latest/source/README.md
- https://forums.gentoo.org/viewtopic.php?p=8823915
- https://github.com/kornelski/cargo-deb
- https://github.com/juliusl/cargo-generate-rpm

---

**Status:** DONE_WITH_CONCERNS
**Summary:** Recommend AppImage built in ubuntu:22.04 (Qt 6.2 baseline) plus per-distro .deb, tray via Rust SNI crate; CXX-Qt min Qt version and exact API unverified. Rerun the empirical 6.2 build test before committing to the target matrix.
**Concerns/Blockers:** CXX-Qt minimum Qt and API details not confirmed (book page 404, release page silent). Many Qt package names marked unverified. Web budget (8) exhausted.
