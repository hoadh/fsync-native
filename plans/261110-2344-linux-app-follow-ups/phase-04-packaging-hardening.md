# Phase 04 — Packaging hardening (git sha, Phase 05 lows, maintainer + license)

## Context
- `apps/linux/packaging/arch/PKGBUILD`, `apps/linux/packaging/{deb,rpm}`, `apps/linux/scripts/{build-release.sh,smoke-install.sh,lib/common.sh}`, `apps/linux/app/build.rs`.
- Lows from `../261106-1414-linux-native-app/reports/code-reviewer-261107-1700-phase-05.md`. Still open after the re-review: L3, L4, L5, L6 (rpm license), L8 (asset list).

## Overview
- Priority P3. Estimate 1.5 d. Independent of phases 01-03, so it gets its own branch.

## Steps
1. **Git sha in PKGBUILD:**
   - In release mode, export `FSYNC_GIT_SHA=$_commit` in `_env()`.
   - In local (tarball) mode there is no `.git`, so `smoke-install.sh` passes the host sha in the env.
   - Check that `--build-info` shows the sha, not `unknown`.
2. **L3:** write RUSTFLAGS with `CARGO_ENCODED_RUSTFLAGS` (unit-separator joined), so paths with spaces survive. Add `--remap-path-prefix=$CARGO_HOME=/cargo`. Test with a BUILDDIR that contains a space.
3. **L4:**
   - Guard against generating `.SRCINFO` with `FSYNC_LOCAL_SRC` set (refuse in `--printsrcinfo` mode, or document it plus a check script).
   - Use a pacman ≥ 6.1 git checksum instead of `SKIP` once `_commit` is a real release commit (template + comment).
   - Add `glibc` and `gcc-libs` to `depends`.
4. **L6 + license/maintainer:**
   - Maintainer is `alex <danghuyhoa@gmail.com>` (user, 2026-11-11). It goes into PKGBUILD `# Maintainer:`, deb `Maintainer:` and rpm `Packager`.
   - License `MIT OR Apache-2.0` (user, 2026-11-11), for the whole repo, not only Linux:
     - Add `LICENSE-MIT` and `LICENSE-APACHE` at the root (copyright line: `alex`).
     - Set `license = "MIT OR Apache-2.0"` in the root `Cargo.toml` and `apps/linux/Cargo.toml` (both are `UNLICENSED` now).
     - Set PKGBUILD `license=('MIT OR Apache-2.0')` and install both files to `/usr/share/licenses/fsync-linux/`.
     - Use the same expression for rpm `License`. Give the deb `copyright` a DEP-5 `License:` stanza for each license.
     - Add a License section to `README.md`. Update the macOS and Windows app metadata if it names a license.
     - Update the asset lists and smoke checks for the new license files.
5. **L8:** define the asset list (desktop file, 3 icons, license) once, in `scripts/lib/assets.list`, and read it in the deb, rpm, PKGBUILD and smoke checks.
6. **L5:** deferred to MVP phase 16 (user, 2026-11-11). Add it to `plans/261013-2029-fsync-native-mvp/phase-16-release-distribution.md` and note it in ADR 014.
7. Run `smoke-install.sh` for all targets, including arch with namcap.

## Todo
- [ ] FSYNC_GIT_SHA in PKGBUILD (both modes) + check
- [ ] L3, L4, L8
- [ ] Maintainer line; MIT OR Apache-2.0 across repo + packages
- [ ] L5 moved to MVP phase 16
- [ ] All smoke targets pass; review >= 9.5; PR

## Risks
The arch smoke takes about 30 min, so run it once at the end. A namcap rule change fails the arch smoke.
