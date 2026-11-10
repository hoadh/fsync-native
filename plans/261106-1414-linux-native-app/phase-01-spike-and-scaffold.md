# Phase 01 — Spike (CXX-Qt on Qt 6.2) + apps/linux scaffold + ADR 014 draft

## Context Links
- [plan.md](plan.md), [brainstorm](../reports/brainstorm-261106-1354-linux-native-app.md), [CXX-Qt research](../reports/researcher-261106-1354-cxx-qt-and-linux-packaging.md) §1, §5
- Separate-workspace example: `spikes/iroh-discovery/Cargo.toml` (`[workspace]` table, own `Cargo.lock`)
- Toolchain pin: `/home/alex/Work/fsync-native/rust-toolchain.toml` (1.99.0; rustup finds it from `apps/linux` via parent dirs)
- CI: none. No `.github/workflows` edits in this plan (Actions billing block); local container scripts are the gate. <!-- red-team 2026-11-06 #14 -->
- Root lockfile: `/home/alex/Work/fsync-native/Cargo.lock` (drift reference for shared crates)

## Overview
- Priority: P1 (gate for the whole plan). Status: Completed. Estimate: 3 d.
- Gates owned here (plan.md Gates): **Qt floor go/no-go**, **GNOME VM provisioned**.
- Prove CXX-Qt (latest 0.10.x, pinned `=`) builds and runs a QObject + QML window against apt Qt 6.2.4 in `ubuntu:22.04`, linked with `fsync-core` (vendored OpenSSL/SQLCipher). Go/no-go decides the distro floor.
- Scaffold the final `apps/linux` workspace (two crates) so later phases only add modules.

## Key Insights
- CXX-Qt min Qt is undocumented; only an empirical build settles it. Qt public ABI is forward-compatible within Qt 6: build on oldest (6.2), run on newer; never the reverse (dev box has Qt 6.12).
- Qt discovery via `QMAKE` env (verify in cxx-qt-build source); Ubuntu path `/usr/lib/qt6/bin/qmake` (or `qmake6`).
- Threading API to verify: `qt_thread()` -> `CxxQtThread<T>` (Send) -> `.queue(|qobj| ...)`. Spike must exercise it from a non-Qt thread, because phase 02 depends on it.
- <!-- red-team 2026-11-06 #13 --> qmlcachegen output (compiled QML) is tied to the Qt version that built it; "forward ABI" covers C++ only. The 6.2-built binary must be run in `fedora:latest` and `archlinux:latest` (newest Qt 6) before the floor is accepted. Fallbacks if it fails there: per-distro builds (deb/rpm built in their own container, PKGBUILD already from source) or disabling qmlcachegen if cxx-qt-build allows it (verify in source).
- AppImage bundles its own Qt: choose 6.2 (apt, same binary) vs pinned 6.8 LTS (aqtinstall, separate build) here, not in 05.
- Qt Quick Controls on 6.2: use `Basic` style only; avoid post-6.2 QML APIs. A `--smoke` mode (load QML, exit 0 when root object exists) under `QT_QPA_PLATFORM=offscreen` is the cheap guard for every later phase and for packaging.

## Requirements
- Functional: `fsync-linux --version`, `fsync-linux --smoke` (offscreen ok). Hello QObject: property `status: QString`, invokable `ping()` that runs on a worker thread and updates `status` via `queue`.
- Non-functional: builds in `ubuntu:22.04` (Qt 6.2.4) and `debian:12` (Qt 6.4.2) containers and on Omarchy (Qt 6.12); the ubuntu-2204 binary runs `--smoke` in `fedora:latest` and `archlinux:latest` runtime containers <!-- red-team 2026-11-06 #13 -->; `fsync-core` linked; no Qt in root workspace.
- Environment: user's existing Ubuntu Desktop (GNOME Wayland) machine reachable (ssh; target asked at phase 01) for phase 03 manual-mode work. <!-- red-team 2026-11-06 #7 --> <!-- Updated: Validation Session 2 - GNOME host is the user's existing Ubuntu Desktop (GNOME) machine -->

## Architecture
Workspace (final layout; this phase creates the bold items, later phases fill the rest):
```
apps/linux/
  **Cargo.toml**            [workspace] members = ["platform", "app"]; shared deps; path deps ../../core/*
  **Cargo.lock**  **README.md**
  platform/                 crate fsync-linux-platform: OS integration + sync logic, NO Qt (unit-testable)
    **Cargo.toml**  **src/lib.rs**
    src/paths.rs            XDG data/config/runtime dirs                         (02)
    src/secrets/            mod.rs, secret_service.rs, file_fallback.rs          (02)
    src/ipc/                mod.rs, protocol.rs, server.rs, client.rs            (02)
    src/autostart.rs  src/notify.rs                                              (02)
    src/clipboard/          mod.rs, detect.rs, formats.rs, filters.rs, echo.rs,
                            wayland_watch.rs, wayland_source.rs,
                            x11_conn.rs, x11_watch.rs, x11_owner.rs        (03, thin: no INCR send)
    src/sync/               mod.rs, outbound.rs, inbound.rs, lanes.rs             (03)
    src/files/              mod.rs, inbox.rs, uri_list.rs, retention.rs          (04a)
  app/                      crate fsync-linux (bin): CXX-Qt bridge + QML
    **Cargo.toml**  **build.rs** (cxx-qt-build QmlModule uri "app.fsync")
    **src/main.rs**         args, single instance, Qt app start (grows in 02)
    src/cli_args.rs                                                            (02)
    **src/bridge/mod.rs**   **src/bridge/hello_object.rs** (spike; deleted in 02)
    src/bridge/             engine_object.rs, engine_worker.rs, delegate_adapter.rs,
                            pairing_object.rs, devices_model.rs, user_messages.rs (02)
                            clipboard_object.rs (03), history_model.rs, transfers_object.rs (04a),
                            settings_object.rs (04b)
    src/tray.rs (ksni)                                                         (02)
    **qml/Main.qml**        + SignInView, PairingView, SasDialog, DevicesView (02),
                            QuickClipWindow (03), HistoryView, OffersView (04a), SettingsView, DevicePicker (04b)
  resources/                app.fsync.Fsync.desktop, icons/hicolor/*, autostart template (02/05)
  scripts/                  **build-in-container.sh**, **run-in-container.sh**, **check-lock-drift.sh**,
                            **container/{ubuntu-2204,debian-12,fedora,archlinux}.Dockerfile**
  packaging/                arch/PKGBUILD, appimage/build-appimage.sh (05)
  tests/e2e/                pair-with-cli.sh (02), clipboard-with-cli.sh (03/04a/04b)
```
Data flow (spike): QML button -> `HelloObject::ping` (GUI thread) -> `std::thread::spawn` -> work -> `qt_thread.queue(|o| o.set_status(..))` -> QML binding updates.

## Related Code Files
Create:
- `/home/alex/Work/fsync-native/apps/linux/Cargo.toml`, `Cargo.lock`, `README.md`
- `/home/alex/Work/fsync-native/apps/linux/platform/Cargo.toml`, `platform/src/lib.rs`
- `/home/alex/Work/fsync-native/apps/linux/app/Cargo.toml`, `app/build.rs`, `app/src/main.rs`, `app/src/bridge/mod.rs`, `app/src/bridge/hello_object.rs`, `app/qml/Main.qml`
- `/home/alex/Work/fsync-native/apps/linux/scripts/build-in-container.sh`, `run-in-container.sh` (runtime-only: install Qt from distro, run a given binary `--smoke`), `check-lock-drift.sh` <!-- red-team 2026-11-06 #13 #15 -->
- `/home/alex/Work/fsync-native/apps/linux/scripts/container/ubuntu-2204.Dockerfile`, `debian-12.Dockerfile`, `fedora.Dockerfile`, `archlinux.Dockerfile` (fedora/arch moved here from 05; 05 reuses them)
- `/home/alex/Work/fsync-native/docs/adr/014-linux-desktop-app.md` (draft)
Modify: none outside `apps/linux` and the ADR (no CI edits). <!-- red-team 2026-11-06 #14 -->

## Implementation Steps
1. Branch `feat/linux-app` from main.
2. Dockerfile `ubuntu-2204`: apt `build-essential perl pkg-config cmake curl ca-certificates git qt6-base-dev qt6-declarative-dev qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts qml6-module-qtquick-window qml6-module-qtqml-workerscript qt6-wayland libgl1-mesa-dev`; rustup installed under `/opt/rust` (`RUSTUP_HOME`/`CARGO_HOME` in `/opt`, world-readable, toolchain from repo pin baked into the image) so any `--user` uid can run cargo <!-- red-team 2026-11-06 #15 -->. Confirm names with `apt-cache policy` (research flags most as unverified); fix list in Dockerfile, not in docs.
3. `debian-12.Dockerfile`: same set (Qt 6.4.2).
4. `build-in-container.sh <image>`: build image, mount repo read-only; bind-mount host-owned cache dirs (`~/.cache/fsync-linux/<image>/{registry,git}` as `CARGO_HOME` overlay, `apps/linux/target-<image>` as target) instead of root-owned named volumes <!-- red-team 2026-11-06 #15 -->; run `cargo build --release --locked --manifest-path apps/linux/Cargo.toml`, then `QT_QPA_PLATFORM=offscreen target/.../fsync-linux --smoke`. Exit code mirrors. Runs as host uid (no root-owned files).
5. Workspace `Cargo.toml`: `[workspace] resolver = "3"`, `members = ["platform", "app"]`, edition 2024; `[workspace.dependencies]` `fsync-core = { path = "../../core/fsync-core" }`, `fsync-proto`, `cxx-qt = "=0.10.x"`, `cxx-qt-lib`, `cxx-qt-build` (exact pins), `tokio`, `tracing`, `thiserror`, `anyhow`. `app` depends on `fsync-core` already (link proof).
6. `hello_object.rs`: `#[cxx_qt::bridge]` QObject `HelloObject` (`QML_ELEMENT`), property `status`, invokable `ping`; impl `Threading`. `main.rs`: parse `--version`/`--smoke`, `QGuiApplication` + `QQmlApplicationEngine` load `qrc:/qt/qml/app/fsync/qml/Main.qml`; `--smoke`: quit 0 after `objectCreated` with non-null root, exit 1 on QML warnings/null (install message handler that counts `QtWarningMsg` from QML). Construct a throwaway `FsyncService::new` in a tempdir in `--smoke` only when `FSYNC_SMOKE_CORE=1` (proves core links/runs in the container).
7. `Main.qml`: `ApplicationWindow` (Controls Basic), Label bound to `hello.status`, Button -> `hello.ping()`.
8. `check-lock-drift.sh`: compare versions of crates shared by `apps/linux/Cargo.lock` and root `Cargo.lock` (`cargo metadata --locked` on both); fail on any mismatch for crates in core's dependency tree (keeps core behavior identical to the tested root build). <!-- red-team 2026-11-06 #15 -->
9. `fedora.Dockerfile` / `archlinux.Dockerfile` (runtime only: distro Qt6 base/declarative/wayland + QML modules) and `run-in-container.sh <image> <binary>`: run the ubuntu-2204 binary with `--version`, `QT_QPA_PLATFORM=offscreen --smoke`, `FSYNC_SMOKE_CORE=1 --smoke`. <!-- red-team 2026-11-06 #13 -->
10. Run on Omarchy (`cargo run -p fsync-linux` under Hyprland, Wayland QPA), then `build-in-container.sh ubuntu-2204` and `debian-12`.
11. Go/no-go (Gate **Qt floor go/no-go**): record versions, build time, binary size, `ldd` summary, fedora/arch run results in ADR 014 draft. No-go on 6.2 -> try 6.4 (debian-12) -> if go, raise floor (Debian 12 / Ubuntu 24.04), update plan.md + phases 05/06 targets. No-go on both -> bundle pinned Qt 6.8 LTS via aqtinstall in AppImage + /opt for deb/rpm (validation 2), re-run this spike against it. <!-- red-team 2026-11-06 #15 -->
12. Decide and record in ADR 014: one binary for deb+rpm+AppImage vs per-distro builds (from step 9 results); AppImage Qt 6.2 vs pinned 6.8 LTS (aqtinstall, exact version + sha256). Phase 05 implements the decision. <!-- red-team 2026-11-06 #13 -->
13. Connect the user's existing Ubuntu Desktop GNOME machine (ask ssh target; no VM provisioning), ssh + `remote`-free manual access; record GNOME version in ADR 014 (Gate **GNOME VM provisioned**). <!-- red-team 2026-11-06 #7 --> <!-- Updated: Validation Session 2 - GNOME host is the user's existing Ubuntu Desktop (GNOME) machine -->
14. ADR 014 draft: context, decisions fixed by brainstorm, spike results table, Platform two-step rollout + emit gate, "untested desktops" list (KDE, Sway live session, X11 desktops, Ubuntu desktop), open items. Update `apps/linux/README.md` with build commands.

## Todo List
- [x] Branch `feat/linux-app`
- [x] ubuntu-2204 + debian-12 Dockerfiles (package names verified)
- [x] `build-in-container.sh` (uid-safe, exit code)
- [x] Workspace + `platform` + `app` crates, exact CXX-Qt pins
- [x] HelloObject with worker-thread `queue` round trip
- [x] `--version`, `--smoke` (QML warning = fail)
- [x] Builds: Omarchy, ubuntu:22.04, debian:12
- [x] Toolchain under /opt, host-owned cache mounts
- [x] Lock drift check
- [x] 6.2 binary `--smoke` in fedora + archlinux containers
- [x] Go/no-go recorded; floor decided; one-binary vs per-distro decided; AppImage Qt decided
- [x] GNOME VM provisioned
- [x] ADR 014 draft

## Success Criteria
- `cargo build --manifest-path apps/linux/Cargo.toml && QT_QPA_PLATFORM=offscreen apps/linux/target/debug/fsync-linux --smoke` -> exit 0 on Omarchy.
- `apps/linux/scripts/build-in-container.sh ubuntu-2204` -> exit 0 (or documented no-go + `debian-12` exit 0).
- `FSYNC_SMOKE_CORE=1` smoke passes in the container (core links and opens a history db).
- `run-in-container.sh fedora <bin>` and `run-in-container.sh archlinux <bin>` -> exit 0, or a recorded per-distro decision. <!-- red-team 2026-11-06 #13 -->
- `check-lock-drift.sh` exit 0.
- GNOME VM: login session reachable, GNOME version recorded.
- Root `cargo test --workspace` unchanged and Qt-free (`cargo metadata` of root has no cxx-qt).
- Manual: Hyprland window opens with Wayland QPA (`QT_DEBUG_PLUGINS=1` shows `libqwayland`), button updates label.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| CXX-Qt needs Qt >= 6.4/6.5 | M x H | Planned fallback floor; decided here, before UI work |
| CXX-Qt API differs from research notes | M x M | Follow version-matched examples in the crate repo tag; pin exact |
| Vendored OpenSSL/SQLCipher fail in 22.04 | L x M | `perl make build-essential` in image; same as netns container |
| Container target dir root-owned / clashes with host target | M x L | `--user $(id -u)`, per-image target dir |
| Rust 1.99 vs CXX-Qt MSRV | L x L | 1.99 is newer than any MSRV |
| 6.2-compiled QML fails on Qt 6.8+ (qmlcachegen) | M x H | Run in fedora/arch here; per-distro builds fallback <!-- red-team 2026-11-06 #13 --> |
| apps/linux lock drifts from root (core deps differ) | M x M | `check-lock-drift.sh` <!-- red-team 2026-11-06 #15 --> |
| No GNOME VM -> GNOME path untested | M x M | Gate before phase 03; else GNOME moves to ADR "untested" (user decides) |

## Security Considerations
- Container mounts repo read-only; no secrets in images. Downloads only from distro mirrors and rustup.

## Rollback
- All new files under `apps/linux/` + ADR draft: revert the branch; root workspace untouched. VM is outside the repo (delete VM).

## Next Steps
- Phase 02 replaces HelloObject with the engine bridge. Floor + build-shape + AppImage Qt decisions feed phase 05. GNOME VM feeds phase 03.
