# Phase 05 implementer report (2026-11-07)

Status: DONE_WITH_CONCERNS (container-verified; Omarchy step 9 and the GNOME LIVE row are manual/open).

## Files
Created (apps/linux): scripts/{release-guard,build-release,package-in-container,smoke-install,derive-runtime-deps}.sh,
scripts/container/{release.Dockerfile,smoke-checks.sh}, packaging/arch/PKGBUILD, packaging/appimage/build-appimage.sh,
packaging/about.hbs, about.toml.
Modified: app/Cargo.toml (deb + generate-rpm metadata), resources/app.fsync.Fsync.desktop (Categories),
README.md, /.gitignore (apps/linux/dist/), docs/adr/014-linux-desktop-app.md, phase-05 todos, plan.md row (In Progress).

## Results (details in ADR 014 "Phase 05 decisions")
- build-release.sh exit 0 -> dist/: deb 10.3 MB, rpm 11.0 MB, AppImage 44.9 MB (42.8 MiB), SHA256SUMS. deb/rpm byte-identical across rebuilds.
- smoke-install: ubuntu-2204 deb, debian-12 deb, ubuntu-2404 deb (extra, t64), fedora rpm, appimage (ubuntu:22.04, no Qt), arch PKGBUILD local mode: all exit 0.
- Negatives: --features e2e-control release -> compile_error (exit 101); FSYNC_ALLOW_TEST_FEATURES=1 build-release.sh exit 1; smoke-install on e2e release binary exit 1.
- namcap: PKGBUILD clean; package no errors (warnings listed in ADR).
- cargo test --workspace (apps/linux, host): exit 0.

## Manual step 9 (Omarchy; NOT run)
```bash
cd /home/alex/Work/fsync-native
git ls-files -z --cached --others --exclude-standard \
  | tar --null -T - --ignore-failed-read --transform 's,^,fsync/,' -czf /tmp/fsync-src.tar.gz
mkdir -p /tmp/fsync-pkg && cp apps/linux/packaging/arch/PKGBUILD /tmp/fsync-pkg/ && cd /tmp/fsync-pkg
FSYNC_LOCAL_SRC=/tmp/fsync-src.tar.gz makepkg -si      # asks for sudo (makedepends + pacman -U); ~15-25 min
# verify:
fsync-linux --version && fsync-linux --build-info       # features must be []
desktop-file-validate /usr/share/applications/app.fsync.Fsync.desktop
# then: start "fsync" from the launcher, Settings -> autostart on, log out/in, tray icon present
```

## Fix round 1 (2026-11-07)
All review/tester items done; details in ADR 014 "Fix round 1". Summary: Wayland check decides on full output
(PASS only on `Failed to create wl_display`; negatives with deleted plugins / libQt6WaylandClient exit 1);
extraction/.desktop failures FAIL; guard = allowlist (mutation x-test-bypass default feature -> exit 1) +
`fsync_core::BUILT_FEATURES` in `--build-info` + RUSTFLAGS/config.toml refusal + PKGBUILD assertion in package();
`-dirty` git_sha + `FSYNC_RELEASE_STRICT=1` (verified refuses); Arch uses system OpenSSL (no $srcdir path, warning gone);
AppImage offscreen smokes on fedora and arch pass; prepare()/--frozen/RUSTUP_TOOLCHAIN/cargo about --offline --fail;
AppImage ships the license report; lows done except digest pins etc. (documented as follow-up).
Re-run: build-release.sh rc 0; smokes ubuntu-2204, debian-12, ubuntu-2404, fedora, appimage (ubuntu/fedora/arch), arch all rc 0;
negatives (env guard exit 1, e2e binary smoke exit 1) re-confirmed; host `cargo test --workspace --locked` 233 passed.
Manual step 9 correction: build in ~/.cache/fsync-pkg, not /tmp:
  mkdir -p ~/.cache/fsync-pkg && cd /home/alex/Work/fsync-native && git ls-files -z --cached --others --exclude-standard | tar --null -T - --ignore-failed-read --transform 's,^,fsync/,' -czf ~/.cache/fsync-pkg/fsync-src.tar.gz
  cp apps/linux/packaging/arch/PKGBUILD ~/.cache/fsync-pkg/ && cd ~/.cache/fsync-pkg && FSYNC_LOCAL_SRC=$PWD/fsync-src.tar.gz makepkg -si
