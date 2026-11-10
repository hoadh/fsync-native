# Phase 05 — Packaging (PKGBUILD, .deb, .rpm, AppImage) + distro install smoke

## Context Links
- [plan.md](plan.md), [phase-01](phase-01-spike-and-scaffold.md) (containers, `--smoke`), [packaging research](../reports/researcher-261106-1354-cxx-qt-and-linux-packaging.md) §3-5
- Guard precedent: `scripts/build-xcframework.sh`, `scripts/build-windows-dll.ps1` (fail if test features present), `docs/code-standards.md` (test-only features)
- cargo-deb, cargo-generate-rpm, linuxdeploy + linuxdeploy-plugin-qt docs (research sources)
- Phase 01 decisions (ADR 014 draft): one binary vs per-distro builds, AppImage Qt (6.2 vs pinned 6.8 LTS); phase 02 `--build-info` + `e2e-control` compile guard; [red team](reports/red-team-261106-1438-plan-review.md) #6 #13 #14 #15

## Overview
- Priority: P2. Status: Completed (container-verified 2026-11-07; GNOME AppImage LIVE 2026-11-07; Omarchy makepkg + autostart 2026-11-09). Estimate: 4 d.
- Build shape follows the phase 01 decision (Gate **Qt floor go/no-go**): default one release binary built in `ubuntu:22.04` feeds .deb and .rpm; per-distro builds if the fedora/arch run failed. AppImage uses the Qt chosen in 01. PKGBUILD builds from source on Arch. <!-- red-team 2026-11-06 #13 --> Install + start smoke in clean containers.

## Key Insights
- Build on oldest Qt (6.2) -> C++ ABI forward-compatible, but compiled QML (qmlcachegen) is Qt-version bound; validity on newer Qt comes only from the 01 container run + this phase's smoke. Never ship the Omarchy (Qt 6.12) build. <!-- red-team 2026-11-06 #13 -->
- PKGBUILD builds against the system Qt: an Arch Qt minor bump can break compiled QML -> `pkgrel` bump + rebuild on every Qt 6 minor update (note in PKGBUILD comment + README). <!-- red-team 2026-11-06 #13 -->
- Runtime deps from `ldd` + `qmlimportscanner` in the container, mapped to packages (`dpkg -S`, `rpm -qf` in a Fedora container), not from memory. Ubuntu 24.04 t64 rename: deb deps need alternatives (`libqt6core6 | libqt6core6t64`).
- One .deb for Ubuntu 22.04+/Debian 12+ if alternatives cover both; else per-distro .deb (decided by smoke).
- AppImage: Wayland QPA not bundled by default -> `EXTRA_PLATFORM_PLUGINS="libqwayland-egl.so;libqwayland-generic.so"`; `QML_SOURCES_PATHS=apps/linux/app/qml`. In containers run with `--appimage-extract-and-run` (no FUSE).
- `--smoke` catches missing QML modules (exit 1 on QML import warnings) without a display (`QT_QPA_PLATFORM=offscreen`).

## Requirements
Functional:
- Artifacts: `fsync-linux_<ver>_amd64.deb`, `fsync-linux-<ver>-1.x86_64.rpm`, `fsync-linux-<ver>-x86_64.AppImage`, `PKGBUILD` (+ `.SRCINFO`).
- Each installs: `/usr/bin/fsync-linux`, `/usr/share/applications/app.fsync.Fsync.desktop`, `/usr/share/icons/hicolor/*/apps/app.fsync.Fsync.*`, `/usr/share/metainfo/app.fsync.Fsync.metainfo.xml` (optional, YAGNI unless trivial), licenses of bundled crates (`cargo about` or `cargo-license` output) in `/usr/share/doc/fsync-linux/`.
- Deps (to verify): Qt6 core/gui/qml/quick, QML modules (QtQuick, Controls, Layouts, Window, WorkerScript), Qt Wayland; Recommends: gnome-keyring | keepassxc (Secret Service), wl-clipboard not needed.
- Release guard <!-- red-team 2026-11-06 #6 -->: (1) `e2e-control` in a release build is a `compile_error!` (phase 02) unless `FSYNC_ALLOW_TEST_FEATURES=1`; `release-guard.sh` fails if that env is set. (2) `cargo tree -e features` check stays for core features `test-hooks`, `test-relay-ca` (visible there). (3) Every packaged binary is run with `--build-info` in `smoke-install.sh`; non-empty test feature list -> FAIL. (4) PKGBUILD `check()` runs the same `--build-info` assertion.
- Smoke matrix (containers, headless): `ubuntu:22.04`, `debian:12`, `fedora:latest` (latest only) <!-- Updated: Validation Session 2 - Fedora latest only -->, `archlinux:latest` (makepkg), AppImage on `ubuntu:22.04` without Qt installed. Each: install, `fsync-linux --version`, `--build-info` (no test features), `QT_QPA_PLATFORM=offscreen fsync-linux --smoke`, `FSYNC_SMOKE_CORE=1` variant, `desktop-file-validate` on the .desktop file. <!-- red-team 2026-11-06 #15 -->
Non-functional: one command builds all artifacts (`apps/linux/scripts/build-release.sh`); reproducible from clean checkout; AppImage size measured and recorded.

## Architecture
```
build-release.sh
  -> release-guard.sh -> check-lock-drift.sh (01)
  -> container ubuntu-2204: cargo build --release --locked -p fsync-linux -> target/release/fsync-linux
       -> cargo deb --no-build        -> dist/*.deb
       -> cargo generate-rpm          -> dist/*.rpm   (auto-req disabled, explicit requires)
       -> linuxdeploy --plugin qt     -> dist/*.AppImage
  -> dist/SHA256SUMS
smoke-install.sh <distro> <artifact> -> fresh container -> install -> --version, --smoke, desktop-file-validate
packaging/arch/PKGBUILD -> makepkg in archlinux container (build from source tarball/git tag)
```

## Related Code Files
Create (under `/home/alex/Work/fsync-native/apps/linux/`):
- `scripts/build-release.sh`, `scripts/smoke-install.sh`, `scripts/release-guard.sh`, `scripts/derive-runtime-deps.sh` (ldd + qmlimportscanner -> package list report)
- (fedora/arch Dockerfiles already created in 01; reused here) <!-- red-team 2026-11-06 #13 -->
- `packaging/arch/PKGBUILD`, `packaging/appimage/build-appimage.sh`, `packaging/appimage/AppRun` (only if needed, else linuxdeploy default)
- `resources/app.fsync.Fsync.metainfo.xml` (optional)
Modify: `app/Cargo.toml` (`[package.metadata.deb]`, `[package.metadata.generate-rpm]` with assets + depends/requires), `resources/app.fsync.Fsync.desktop` (final Exec/Icon/Categories, `StartupWMClass`), `apps/linux/README.md` (build/install, Arch Qt-bump rebuild note), `/home/alex/Work/fsync-native/.gitignore` (`apps/linux/dist/`). No CI workflow edits. <!-- red-team 2026-11-06 #14 -->

## Implementation Steps
1. `release-guard.sh` (refuse `FSYNC_ALLOW_TEST_FEATURES`, `cargo tree` core test features) called first by `build-release.sh`; `smoke-install.sh` asserts `--build-info`. <!-- red-team 2026-11-06 #6 -->
2. `derive-runtime-deps.sh` in ubuntu-2204 and fedora containers: `ldd` -> owning packages; `qmlimportscanner -rootPath app/qml -importPath <qt qml dir>` -> QML module packages. Paste result into Cargo metadata; keep the report in the phase notes / ADR 014.
2b. Packaging tools pinned <!-- red-team 2026-11-06 #15 -->: `cargo install --locked --version <x.y.z> cargo-deb cargo-generate-rpm` (and `cargo-about` if used) inside the image, versions recorded in the Dockerfile.
3. cargo-deb metadata (assets: binary, desktop, icons, docs; `depends` with t64 alternatives; `recommends`). Build in container.
4. cargo-generate-rpm metadata (`auto-req = "disabled"`, explicit `qt6-qtbase`, `qt6-qtdeclarative`, `qt6-qtwayland`).
5. AppImage script (Qt per 01 decision; 6.8 LTS via `aqtinstall` pinned version + sha256 if chosen): linuxdeploy + plugin-qt AppImages pinned by release tag/continuous build id + sha256 (download into a fresh dir, verify hash), env `QMAKE`, `QML_SOURCES_PATHS`, `EXTRA_PLATFORM_PLUGINS`; desktop + icon; run with `APPIMAGE_EXTRACT_AND_RUN=1`. Measure size.
6. PKGBUILD: `depends=(qt6-base qt6-declarative qt6-wayland)`, `optdepends=(gnome-keyring keepassxc)`, `makedepends=(cargo qt6-tools git)`, `options=(!lto)` if vendored C libs object; `build()` runs `cargo build --release --locked --manifest-path apps/linux/Cargo.toml`; `check()` runs platform tests + `--build-info` assertion (no test features) <!-- red-team 2026-11-06 #6 -->. Source pinned `git+https://...#commit=<sha>` (or `#tag=` + `sha256sums`), not a moving branch <!-- red-team 2026-11-06 #15 -->. Comment: rebuild on Qt minor bump <!-- red-team 2026-11-06 #13 -->. `namcap` clean.
7. `smoke-install.sh` matrix; on dep failure fix metadata, not the smoke.
8. Desktop file: `Exec=fsync-linux %F` (drop files on launcher -> `--send`? only if trivial: map `%F` to `--send`), `Icon=app.fsync.Fsync`, `Categories=Utility;Network;`, `X-GNOME-Autostart-enabled` only in autostart copy.
9. Omarchy: install via `makepkg -si`, start from launcher, autostart toggle -> relogin -> app in tray.
10. Update README + ADR 014 draft with sizes, deps, matrix results.

## Todo List
- [x] release guard (env refusal, cargo tree, `--build-info` in smoke + PKGBUILD check)
- [x] packaging tools pinned (`--locked --version`), PKGBUILD `#commit=` (placeholder var `_commit`, refused until filled; `FSYNC_LOCAL_SRC` local mode for smoke)
- [x] runtime deps derived (deb + rpm) and recorded
- [x] .deb metadata + build
- [x] .rpm metadata + build
- [x] AppImage (wayland plugins, pinned tools, size measured)
- [x] PKGBUILD + namcap
- [x] smoke matrix all PASS
- [x] desktop file validated (all targets)
- [x] autostart on Omarchy (manual, step 9) — 2026-11-09: `makepkg` in ~/.cache/fsync-pkg built 0.1.0-1 (package() build-info `"features":[]`), installed with `pacman -U`; desktop-file-validate ok; Settings toggle wrote ~/.config/autostart/app.fsync.Fsync.desktop; after relogin uwsm started `app-app.fsync.Fsync@autostart.service` running `/usr/bin/fsync-linux --background` 3 s after Hyprland, tray icon shown. Follow-up: release-mode PKGBUILD should pass `FSYNC_GIT_SHA` (local tarball reports `git_sha` "unknown")
- [x] README / ADR updated

## Success Criteria
- `apps/linux/scripts/build-release.sh` -> `dist/` with deb, rpm, AppImage, SHA256SUMS; exit 0.
- `apps/linux/scripts/smoke-install.sh ubuntu-2204 dist/*.deb`, `debian-12 dist/*.deb`, `fedora dist/*.rpm`, `appimage dist/*.AppImage`, `arch packaging/arch/PKGBUILD` -> each exit 0 (`--version` prints, `--smoke` 0).
- Negative tests: release build with `--features e2e-control` fails to compile; `FSYNC_ALLOW_TEST_FEATURES=1 build-release.sh` exits 1; `smoke-install.sh` on a debug e2e binary exits 1 (`--build-info`). <!-- red-team 2026-11-06 #6 -->
- `desktop-file-validate resources/app.fsync.Fsync.desktop` clean.

## Pending Live Verification
<!-- red-team 2026-11-06 #14 --> Real Ubuntu 22.04 desktop -> ADR 014 "untested" (container smoke only).
- [x] LIVE: AppImage on GNOME VM QPA — 2026-11-07 Ubuntu 26.04 / GNOME 50.1: bundled Qt 6.2 uses xcb on GNOME by design (wayland forced also works); decision keep xcb. Super+Alt+C send and Super+Alt+V paste pass under the AppImage's Qt 6.2 xcb runtime — evidence: ADR 014 distro-check table

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Qt 6.2-built binary misbehaves on Qt 6.8+ (qmlcachegen, private API use by cxx-qt-lib) | M x H | Decided in 01 by container run; smoke on fedora/arch here; per-distro builds fallback |
| Arch Qt bump breaks installed package | M x M | Rebuild-on-bump note; `--smoke` in PKGBUILD `check()` |
| Test feature shipped | L x H | compile_error + `--build-info` in smoke and PKGBUILD |
| Wrong package names / t64 rename | H x M | Derive from ldd/dpkg -S; smoke installs with apt resolver |
| AppImage misses wayland/QML plugins | M x M | Explicit env; `--smoke` in container without system Qt |
| linuxdeploy / packaging tool drift or tampering | L x H | Pinned versions + sha256; `cargo install --locked --version` |
| AppImage large (~70-120 MB) | H x L | Accept; deb/rpm are small |

## Security Considerations
- Release builds exclude test features (compile guard + `--build-info` check). Tool downloads hash-pinned. No signing keys in repo (package signing is MVP phase 16).
- Packaged binary has no setuid/capabilities; files 0644/0755.

## Rollback
- Packaging files only; revert commits. No runtime code changes except desktop file.

## Next Steps
- Phase 06 uses the AppImage/PKGBUILD build for cross-OS runs; signing + repo hosting -> MVP phase 16.
