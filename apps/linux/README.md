# fsync for Linux

Qt6/QML desktop app over `fsync-core`, bridged with CXX-Qt. Design: [ADR 014](../../docs/adr/014-linux-desktop-app.md).
Own Cargo workspace: the root `cargo test --workspace` never needs Qt.

| Path | Contents |
|---|---|
| `platform/` | `fsync-linux-platform`: OS integration and sync logic, no Qt (unit-testable) |
| `app/` | `fsync-linux` binary: CXX-Qt bridges (`src/bridge/`, all in one directory), QML (`qml/`) |
| `scripts/` | container build and run scripts, release guard, `build-release.sh`, `smoke-install.sh`, lock drift and asset checks, `lib/assets.list`, Dockerfiles in `container/` |
| `packaging/` | `arch/PKGBUILD`, `appimage/build-appimage.sh`, `deb/copyright`, `about.hbs` (third-party license report) |

## Prerequisites (host build)
Rust from `rust-toolchain.toml` (via rustup), Qt 6 dev packages with `qmake6`, `qmlcachegen`
and the QtQuick/Controls QML modules. On Arch/Omarchy: `qt6-base qt6-declarative qt6-wayland`.
Optional: `lld` (otherwise the gold linker is used).

## Build and run
```bash
cd apps/linux
QMAKE=/usr/bin/qmake6 cargo build
./target/debug/fsync-linux                      # window
QT_QPA_PLATFORM=offscreen ./target/debug/fsync-linux --smoke   # exit 0 = QML loads cleanly
FSYNC_SMOKE_CORE=1 QT_QPA_PLATFORM=offscreen ./target/debug/fsync-linux --smoke
cargo test -p fsync-linux-platform
```

## Release build (oldest supported distro)
```bash
apps/linux/scripts/build-in-container.sh ubuntu-2204   # -> apps/linux/target-ubuntu-2204/release/fsync-linux
apps/linux/scripts/run-in-container.sh fedora    apps/linux/target-ubuntu-2204/release/fsync-linux
apps/linux/scripts/run-in-container.sh archlinux apps/linux/target-ubuntu-2204/release/fsync-linux
apps/linux/scripts/check-lock-drift.sh                 # fsync-core deps equal to the root lock
```
Needs Docker (user in the `docker` group). Cargo caches live in `~/.cache/fsync-linux/<image>`.

## Release packages and install
One command builds every artifact in the `ubuntu:22.04` container (Qt 6.2.4, glibc 2.35; nothing
is installed on the host, needs Docker):
```bash
apps/linux/scripts/build-release.sh   # -> apps/linux/dist/: .deb, .rpm, .AppImage, SHA256SUMS
```
It first runs `release-guard.sh` (refuses `FSYNC_ALLOW_TEST_FEATURES`; an allowlist: any feature
other than the shipped set on any crate of this repo, core included, fails the build) and the lock
drift check, refuses `RUSTFLAGS` or a cargo `config.toml` in the container, and asserts that the
binary's `--build-info` (generated from the features actually compiled in, core `test-hooks` /
`test-relay-ca` included) lists none. `--build-info` shows `git_sha` with a `-dirty` suffix when the
tree has uncommitted changes. **For a real release run `FSYNC_RELEASE_STRICT=1 build-release.sh`**:
a dirty tree is then refused. Packaging tools are pinned in
`scripts/container/release.Dockerfile` (cargo-deb, cargo-generate-rpm, cargo-about by
`cargo install --locked --version`; linuxdeploy and its Qt plugin by release tag + sha256).

| Format | Install | Notes |
|---|---|---|
| `.deb` (Ubuntu 22.04+, Debian 12+; one file) | `sudo apt install ./fsync-linux_0.1.0-1_amd64.deb` | Qt from the distro; Recommends a Secret Service (gnome-keyring, KeePassXC, KWallet) |
| `.rpm` (Fedora latest) | `sudo dnf install ./fsync-linux-0.1.0-1.x86_64.rpm` | Qt from the distro |
| AppImage | `chmod +x fsync-linux-0.1.0-x86_64.AppImage && ./fsync-linux-0.1.0-x86_64.AppImage` | bundles Qt 6.2 + Wayland plugins (~43 MB); needs the host's GL, fontconfig, harfbuzz, freetype and xkbcommon libraries (any desktop has them) |
| Arch / Omarchy | `makepkg -si` with the PKGBUILD copied to `~/.cache/fsync-pkg` (see below; never build inside the repo) | builds from source against the system Qt |

**Arch PKGBUILD.** It links the system OpenSSL (`OPENSSL_NO_VENDOR=1`, `depends=openssl`); the deb/rpm keep a vendored copy (below). `source` is a git URL pinned to one commit: before a release set `_commit` to the
40-hex release sha, run `updpkgsums` (pacman >= 6.1 checksums the git source instead of `SKIP`)
and regenerate `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`); a placeholder or branch is refused,
and so are `--printsrcinfo` / `--source` / `--allsource` while `FSYNC_LOCAL_SRC` is set. Until a
commit exists, build the working tree instead: make a tarball whose top directory is `fsync/` and
run (from the repo root; the helper adds `-dirty` for uncommitted changes)
`FSYNC_GIT_SHA=$(source apps/linux/scripts/lib/common.sh && fsync_git_sha .) FSYNC_LOCAL_SRC=/abs/fsync-src.tar.gz makepkg -si`
(`scripts/smoke-install.sh arch packaging/arch/PKGBUILD` does exactly this in an Arch container,
in a non-default build dir). `check()` runs the platform tests and both `--smoke` modes;
`package()` asserts that `--build-info` lists no test features and shows the expected `git_sha`
(the pinned commit, or `FSYNC_GIT_SHA` / `local` for a tarball), so `--nocheck` cannot skip it.
Build paths are remapped (`$srcdir` -> `/build`, `$CARGO_HOME` -> `/cargo`; Rust through
`CARGO_ENCODED_RUSTFLAGS`, C/C++ through `-ffile-prefix-map`). A `BUILDDIR` or `CARGO_HOME` with
whitespace is refused (cc-rs splits `CFLAGS` on whitespace, the C/C++ remap could not work).
makepkg runs as you, so your `~/.cargo/config.toml` (`[build] rustc-wrapper`, `[env]`, `[profile]`)
applies; build release packages without one, or with `CARGO_HOME` pointing at a clean directory.

**Installed files and licenses.** `scripts/lib/assets.list` is the one list of checked-in files a
package installs (desktop file, three icons, `LICENSE-MIT`, `LICENSE-APACHE` in
`/usr/share/licenses/fsync-linux/`). The PKGBUILD and the smoke checks read it; cargo-deb only reads
`app/Cargo.toml`, so `scripts/check-assets.sh` fails the release build when the deb/rpm blocks drift
from it. License `MIT OR Apache-2.0` (PKGBUILD `license`, rpm `License`, the DEP-5 deb `copyright`
in `packaging/deb/`). Maintainer `alex <danghuyhoa@gmail.com>` (PKGBUILD, deb `Maintainer`; rpm
`Vendor`, because cargo-generate-rpm has no `Packager` key).
**On every Qt 6 minor update in Arch (6.12 -> 6.13, ...) bump `pkgrel` and rebuild** the package:
it compiles QML (qmlcachegen) against the installed Qt, and the supported state is "built against
the Qt that is installed".

**Vendored OpenSSL in deb/rpm/AppImage.** SQLCipher uses a vendored OpenSSL, whose compiled-in
`MODULESDIR`/`ENGINESDIR` point into the build tree (`/src/apps/linux/target-ubuntu-2204/...`, a
root-only creatable path on user machines) and `OPENSSLDIR` is `/usr/local/ssl`. Accepted because only root can create those paths (not verified whether anything
loads a provider or engine by name). The Arch package links the system OpenSSL instead. Do not
build the PKGBUILD in a world-writable directory such as `/tmp`.

**Install smoke** (clean containers, headless; each exits 0 only if `--version`, `--build-info`
without features, `--smoke` and `FSYNC_SMOKE_CORE=1 --smoke` pass and `desktop-file-validate` is
clean; the AppImage runs on a host without Qt):
```bash
apps/linux/scripts/smoke-install.sh ubuntu-2204 apps/linux/dist/fsync-linux_0.1.0-1_amd64.deb  # also ubuntu-2404, debian-12
apps/linux/scripts/smoke-install.sh fedora      apps/linux/dist/fsync-linux-0.1.0-1.x86_64.rpm
apps/linux/scripts/smoke-install.sh appimage    apps/linux/dist/fsync-linux-0.1.0-x86_64.AppImage  # also appimage-fedora, appimage-arch
apps/linux/scripts/smoke-install.sh arch        apps/linux/packaging/arch/PKGBUILD   # ~20 min: builds from source
apps/linux/scripts/derive-runtime-deps.sh ubuntu-2204   # ldd + qmlimportscanner -> owning packages
```
Not covered: real desktops (container smoke only; see ADR 014 "untested").

## Clipboard
Automatic sync on Hyprland, Sway, KDE Plasma, COSMIC (Wayland data-control) and X11 sessions;
manual everywhere else. Details and limits: [ADR 014](../../docs/adr/014-linux-desktop-app.md).

| Session | Backend | Behaviour |
|---|---|---|
| Wayland with ext/zwlr data-control | auto | copies are published, received clips are written |
| X11 | auto (thin: INCR receive only, entries over the server's request size are skipped) | same |
| GNOME Wayland (no data-control) | manual | `fsync-linux --clip-send` / `--clip-paste` open a small focused window |
| any | `FSYNC_CLIPBOARD_BACKEND=manual` | disables background access without a rebuild |

- Core setting `clip_mode = Manual` holds received clips as "pending" until `--clip-paste` (tray:
  "Paste received clip") and publishes only on `--clip-send` ("Send clipboard now").
- Hotkeys and "send to": see [Keyboard shortcuts](#keyboard-shortcuts-and-send-to) below.
- Privacy: copies with a password-manager hint, from a listed app (built-in list plus the
  `denylist` setting) or from fsync itself are never read or sent. On Wayland the copying app is
  unknown (only the focused window on Hyprland/Sway), so a manager without a hint can be synced:
  use Manual mode or the denylist. Received clips carry the hint so clipboard managers skip them.
- Files: a copied `text/uri-list` is published only together with a file-manager type and never
  from `~/.ssh`, `~/.gnupg`, keyrings, the password store or fsync's own directories.
- The clip fsync wrote disappears from the clipboard when fsync quits (it is served by the app).
- Settings tab: clip mode, retention days / size (also for the file inbox; a large clip you download
  yourself is removed by age only), largest clip, auto-accept files, app denylist, shortcut help.
- Tests: `cargo test -p fsync-linux-platform` (backends against a private headless sway and
  Xvfb, skipped when they are missing) and `tests/e2e/clipboard-with-cli.sh` (needs `sway`,
  `wl-clipboard`, `xorg-server-xvfb`, `xclip`; never touches your real clipboard).

## Keyboard shortcuts and send to
fsync never grabs keys: your desktop runs a command and the command reaches the running app
(`fsync-linux --help` prints the same lines; the Settings tab has copy buttons). Suggested keys are
Super+Alt+C (send) and Super+Alt+V (paste); never Ctrl+Alt, it is AltGr on many layouts.

| Desktop | Where | Lines |
|---|---|---|
| Hyprland (Omarchy) | `~/.config/hypr/bindings.conf` | `bind = SUPER ALT, C, exec, fsync-linux --clip-send`<br>`bind = SUPER ALT, V, exec, fsync-linux --clip-paste` |
| Sway | `~/.config/sway/config` | `bindsym Mod4+Mod1+c exec fsync-linux --clip-send`<br>`bindsym Mod4+Mod1+v exec fsync-linux --clip-paste` |
| KDE Plasma | System Settings > Keyboard > Shortcuts > Add Command | `fsync-linux --clip-send`, `fsync-linux --clip-paste` |
| GNOME | Settings > Keyboard > Custom Shortcuts | `fsync-linux --clip-send`, `fsync-linux --clip-paste` (a small window opens and takes the focus) |

- Send files: drop them on a device row (Devices tab), or on the window to pick a device, or run
  `fsync-linux --send FILE...` (opens the device picker). Only files under your home folder or on
  removable media are sent, never `~/.ssh`, keyrings, browser profiles or fsync's own data. A
  refused `--send` exits 1 with `path-protected`.
