# Code review: phase 05 (packaging and distro smoke), 2026-11-07

Score: 8.3 / 10. Critical: 0. High: 1. Medium: 6. Low: 10. Below the 9.5 bar.

## Scope
apps/linux: scripts/{release-guard,build-release,package-in-container,smoke-install,derive-runtime-deps}.sh,
scripts/container/{release.Dockerfile,smoke-checks.sh}, packaging/arch/PKGBUILD, packaging/appimage/build-appimage.sh,
about.toml, packaging/about.hbs, app/Cargo.toml (deb/rpm metadata), resources/app.fsync.Fsync.desktop, README, .gitignore.
About 700 LOC. No file is over 500 lines. shellcheck (koalaman/shellcheck:stable, -S warning): scripts clean;
the PKGBUILD only gets SC2164 (`cd` without `||`), which is harmless under makepkg errexit.

What I checked: the shipped dist/ artifacts (deb control and data, `--build-info` of the deb binary, readelf
NEEDED, GLIBC symbol floor 2.34, AppImage contents). I also ran the AppImage Wayland check from smoke-checks.sh in a
clean ubuntu:22.04 container.

## Critical
None. I found no path that puts a test feature into a release artifact without someone first changing the
build environment. Details are in M2.

## High
H1. The Wayland QPA check in AppImage mode passes no matter what. `scripts/container/smoke-checks.sh:62-66`
- With `QT_DEBUG_PLUGINS=1`, more than 8 QFactoryLoader lines contain "wayland" before the verdict line.
  `| head -8` drops the verdict line, so the `Could not (find|load)` grep never sees it.
- Reproduced on dist/fsync-linux-0.1.0-x86_64.AppImage. The full output contains
  `qt.qpa.plugin: Could not load the Qt platform plugin "wayland" in "" even though it was found.`, yet the
  check printed VERDICT=PASS.
- The logic is also wrong without the truncation. With no compositor, a working plugin and a plugin whose
  dependencies are missing both end in "Could not load ... even though it was found". So the check cannot tell
  success from failure, and ADR 014 decision 16 and the smoke-matrix row rest on it.
- Fix: decide on the full output, not the `head` excerpt. Pass if `Failed to create wl_display` appears (the
  plugin's code ran). Fail if `Cannot load library .*qwayland` or `Could not find the Qt platform plugin`
  appears. Use `head` for display only.

## Medium
M1. Release artifacts from a dirty tree carry a clean-looking provenance. `build-release.sh:32` and `app/build.rs`
(git_sha)
- The current dist/*.deb reports `{"features":[],"git_sha":"c25c413",...}`. Commit c25c413 does not contain
  apps/linux at all, because all of apps/linux is untracked.
- SOURCE_DATE_EPOCH also comes from that commit. A release cannot be traced back to its source.
- Fix: release-guard should refuse when `git status --porcelain` is non-empty, unless an explicit
  `FSYNC_RELEASE_DIRTY_OK=1` dev escape is set. Alternatively, build.rs can use `git describe --always --dirty`.

M2. Core test features are invisible to `--build-info`, and the guards do not cover the build environment.
`app/src/build_info.rs`, `release-guard.sh:18`, `PKGBUILD build()`
- `features` only reports the app and platform `e2e-control`. Core `test-hooks` and `test-relay-ca` are checked
  by exactly one layer: a `cargo tree` run on the HOST against the manifest.
- That check cannot see what the container or makepkg build actually got. RUSTFLAGS
  `--cfg feature="test-relay-ca"`, or a `config.toml` in the bind-mounted CARGO_HOME
  (`~/.cache/fsync-linux/ubuntu-2204/config.toml`), would enable it unseen.
- The Arch path runs no tree guard at all, and `makepkg --nocheck` skips the only `--build-info` assertion.
- Fix:
  - Have fsync-core export `BUILT_FEATURES` and merge it into `--build-info`.
  - In package-in-container.sh, assert that RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS are empty and that
    `$CARGO_HOME/config.toml` is absent.
  - Move the PKGBUILD `--build-info` assertion into `package()` so `--nocheck` cannot skip it.

M3. Vendored OpenSSL hardcodes build-tree paths. This is the root cause of the "$srcdir reference" namcap
warning, which ADR 014 calls "source not found, harmless".
- The shipped deb binary contains
  `MODULESDIR: "/src/apps/linux/target-ubuntu-2204/release/build/openssl-sys-*/out/openssl-build/install/lib/ossl-modules"`
  and the matching `ENGINESDIR`. OPENSSLDIR is /usr/local/ssl.
- `-ffile-prefix-map` does not rewrite `-D` string macros, so the remap cannot remove these paths.
- In the deb/rpm, `/src` can only be created by root, so the risk is low there.
- In the Arch package, the path is `$srcdir/target/...`. The implementer's manual step 9 builds in `/tmp/fsync-pkg`.
  After cleanup, any local user can recreate that directory and plant a provider or engine `.so`. It gets loaded
  if anything ever loads a provider or engine by name (config or code). This is the CVE-2019-1552 class.
- Fix: in the PKGBUILD, export `OPENSSL_NO_VENDOR=1` and add `depends+=(openssl)`, so the build links Arch's
  libcrypto (pacman then delivers security updates). Don't build in /tmp. Correct the ADR text.

M4. The AppImage bundles 22.04 system libraries that host GL drivers share, and its smoke only runs on the build
distro. `smoke-install.sh:69-79`
- The AppDir has libglib-2.0, libdbus-1, libsystemd, libgcrypt, libzstd, liblzma, libffi, libgomp and krb5.
- Host Mesa (libEGL/libGL, excluded) loads into the same process and resolves these SONAMEs to the older bundled
  copies. Newer hosts (Arch, Fedora) are where this breaks.
- The only AppImage smoke runs on ubuntu:22.04, which has the same library versions as the bundle. It is the host
  least able to show the problem.
- Fix: add an offscreen AppImage smoke on fedora:latest and archlinux (base libs only). Keep the LIVE GNOME row
  open until then.

M5. The PKGBUILD departs from Arch Rust packaging conventions.
- There is no `prepare()` running `cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"`, and
  build() uses `--locked`, not `--frozen`. Crates are downloaded inside build().
- Nothing sets `RUSTUP_TOOLCHAIN=stable`. With rustup as the `cargo` provider, the repo's rust-toolchain.toml
  (1.99.0) makes rustup download a toolchain mid-build.
- `cargo about generate` runs without `--offline`/`--frozen` (network) and without `--fail`. Without `--fail`, a
  crate whose license cannot be read or synthesized is skipped silently. The same `--fail` gap applies in
  package-in-container.sh:24.

M6. In AppImage mode, desktop-file validation can be skipped silently. `smoke-checks.sh:57-59,73`
- If `--appimage-extract` fails (its output goes to /dev/null), `desktop_to_check` is empty.
- Step 4 is then skipped with no FAIL.
- Fix: call `bad` when no .desktop file is found in AppImage or installed mode.
- Also, `APPIMAGE_EXTRACT_AND_RUN=0` does not turn extract-and-run off: the runtime only checks whether the
  variable is set. Use `env -u`.

## Low
- L1 `smoke-install.sh:43-48,54,77`: `$name` (the artifact basename) is interpolated unquoted into `bash -c` and
  into `--mount dst=`. A file name with spaces, `;` or `,` breaks the script or runs as shell. Fix: pass the name
  with `-e NAME=...` and use a fixed in-container path such as `/pkg/artifact.deb`.
- L2 PKGBUILD:75: the version check is a substring match (`*"$pkgver"*`), so 10.1.0 passes for 0.1.0. Compare
  against `fsync-linux $pkgver` exactly.
- L3 PKGBUILD:50-52: `--remap-path-prefix=$srcdir=/build` goes into space-split RUSTFLAGS, so a BUILDDIR containing
  spaces breaks it. Also, `$HOME/.cargo/registry` paths (with the builder's username) stay in panic strings. Add
  `--remap-path-prefix=$CARGO_HOME=/cargo`.
- L4 PKGBUILD: the sources depend on the environment (`FSYNC_LOCAL_SRC`). A .SRCINFO generated with that variable
  set by mistake would publish a `file://` source. The `#commit=` pin is SHA-1 only; pacman >= 6.1 can checksum
  git sources, so use a real sha256 instead of SKIP. `depends` omits glibc and gcc-libs (namcap "implicit").
- L5 release.Dockerfile / build.Dockerfile:
  - The linuxdeploy hashes are trust-on-first-use; there is no upstream signature.
  - `curl --proto '=https'` has no `--proto-redir '=https'`. The sha256 check mitigates this.
  - Base images use floating tags (`ubuntu:22.04`, `fedora:latest`, `archlinux:latest`), not digests, and the
    rustup-init script is fetched unpinned.
  - Pin the base image by digest for releases.
- L6 Deb Depends has no Qt lower bound, e.g. `libqt6core6 (>= 6.2.4) | libqt6core6t64`. No supported distro
  needs it, but it documents the floor. The rpm metadata has no `license` or `url`, and License comes out as
  "UNLICENSED", which rpmlint rejects. This is a placeholder issue.
- L7 build-release.sh: no trap. A failed run leaves a partial dist/ (plus stage/) next to an old-looking layout.
  SHA256SUMS is unsigned.
- L8 DRY:
  - The toolchain `sed` is copied in 3 scripts.
  - `.qmlls.ini` creation and the docker-run block are copied between build-in-container.sh and build-release.sh.
  - The asset list (desktop file, 3 icons, license) exists in 4 places: deb, rpm, PKGBUILD, smoke-checks.
  - The `/apps/linux/dist/` ignore belongs in apps/linux/.gitignore, which already holds `/target-*/`.
- L9 derive-runtime-deps.sh:26,29,33: OWNER is set and passed but never used (dead).
- L10 smoke-install.sh arch: `namcap ... || true` means namcap regressions never fail the run. The "namcap clean"
  claim depends on someone reading the log by hand.

## Placeholders (known open user question, not code defects)
Maintainer `fsync <noreply@fsync.invalid>`, `license=('LicenseRef-fsync-proprietary')` with no LICENSE file
installed (namcap/Arch require one for LicenseRef), Cargo `UNLICENSED`, deb copyright "all rights reserved".

## Positive
- The layered test-feature guard holds for every normal path: env refusal, `cargo tree` on the resolved
  `-p fsync-linux` graph, compile_error!, and `--build-info` checked on the stripped binary before packaging.
  The e2e builds use a separate target dir.
- linuxdeploy and its Qt plugin are verified with sha256 before anything runs. They are extracted, not run
  through FUSE.
- The packaging tools are pinned with `--locked --version`, and the Dockerfile asserts the binaries exist.
- Docker runs use host uid, a read-only repo mount and explicit env (nothing leaks from the host env).
- Runtime deps come from readelf, ldd and the scanner. They are minimal and correct: Quick is not NEEDED, the
  GLIBC floor of 2.34 matches the binary, and Qt private ABI is not used.
- The deb/rpm builds are reproducible (SOURCE_DATE_EPOCH). The xdg-shell bundle is asserted at build time.

## Recommended actions (to reach 9.5)
1. H1: rewrite the Wayland verdict (full output; `Failed to create wl_display` = pass).
2. M1: refuse dirty-tree releases or mark the sha `-dirty`.
3. M2: core features in `--build-info`, an env/config assertion in the container, `--build-info` in PKGBUILD `package()`.
4. M3: `OPENSSL_NO_VENDOR=1` + `depends+=(openssl)` in the PKGBUILD; correct the ADR "harmless" note.
5. M4: AppImage offscreen smoke on fedora/arch.
6. M5/M6: prepare()/--frozen/RUSTUP_TOOLCHAIN, `cargo about --fail`, fail on a missing .desktop.

## Unresolved questions
- Is github.com/hoadh/fsync-native public? If it is private, the PKGBUILD release mode (anonymous https git
  clone) fails for every AUR user.
- Does SQLCipher or any dependency load an OpenSSL provider or engine by name? That decides whether M3 is
  exploitable today or is defense in depth.
- Is it acceptable for the Arch package to link the system OpenSSL while the deb/rpm stay vendored (different
  crypto library builds across formats)?
- Final product license and maintainer identity (placeholders).
