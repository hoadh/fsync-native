# Code review: phase 05 re-review after fix round 1

Score 9.5/10. Critical 0, high 0, medium 0, low 8, info 1. APPROVED.

## Prior findings (re-run, not trusted)
- H1 Wayland check: fixed. Real AppImage smoke passes on `Failed to create wl_display`; negatives (wayland plugins deleted, libQt6WaylandClient deleted) FAIL.
- D1/M2: allowlist guard fails on `x-test-bypass` default in fsync-core and `sneaky` in fsync-proto; `fsync_core::BUILT_FEATURES` reports cargo features and RUSTFLAGS cfg injection; chained into `--build-info`.
- M1/D4: dist binaries report `c25c413-dirty`; `FSYNC_RELEASE_STRICT` logic correct (not re-run).
- M3: `OPENSSL_NO_VENDOR=1` + `depends=openssl` verified against openssl-sys/libsqlite3-sys build scripts; arch smoke not re-run.
- M4: appimage-fedora/arch targets exist (not re-run). M5, M6, D3 and lows L1, L2, L7, L9, L10 fixed. No functional regressions.

## Low (open after re-review)
1. ADR decision 14 / README said `check()` for the build-info assertion (now `package()`). Fixed by controller.
2. README Arch row built inside the repo (dirties tree). Fixed by controller: doc points to `~/.cache/fsync-pkg`; arch build outputs gitignored.
3. `package-in-container.sh:66-71` config check misses `CARGO_TARGET_<triple>_RUSTFLAGS`, `/src/apps/.cargo/config.toml`, `/src/.cargo/config`, `/.cargo/config.toml`.
4. `app/build.rs:81-85` git fallback: walks parent repos in tarball mode (use `GIT_CEILING_DIRECTORIES`), unstaged edits don't rerun build script in dev, prefer `--no-optional-locks`.
5. `PKGBUILD:63` replaces makepkg.conf RUSTFLAGS (drops frame pointers) and splits on spaces; `CARGO_ENCODED_RUSTFLAGS` safer.
6. `smoke-checks.sh:138` interpolates path into `bash -c` (quote breaks it); pass via env.
7. `"features":[]` substring checks; `jq -e '.features==[]'` sturdier.
8. `scripts/lib/common.sh` lacked shellcheck directive. Fixed by controller.

## Info
- I1: runtime Wayland check cannot see missing xdg-shell without a compositor; build-time assert in `build-appimage.sh:39-40` covers it; GNOME LIVE row is the real proof.
- Accepted follow-ups: digest-pinned base images, TOFU linuxdeploy hashes, SHA-1 git pin, repeated asset list. Maintainer/license placeholders excluded.

## Unresolved questions
- Is github.com/hoadh/fsync-native public (PKGBUILD release mode)?
- Arch on system OpenSSL vs vendored in deb/rpm/AppImage: acceptable?
