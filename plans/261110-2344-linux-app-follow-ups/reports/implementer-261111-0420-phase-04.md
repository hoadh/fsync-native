# Implementer report: phase 04 (packaging hardening), 2026-11-11

Branch `fix/packaging-hardening` (from main 4e7b33b). Not pushed, not merged.

## Done
| Item | Change |
|---|---|
| Git sha | PKGBUILD `_gitsha`: `${_commit:0:7}` (release) or `FSYNC_GIT_SHA` from the env, else `local` (tarball). `_env()` exports `FSYNC_GIT_SHA`; `package()` asserts `"git_sha":"$_gitsha"`. `smoke-install.sh arch` passes the host sha (`fsync_git_sha`) and `FSYNC_EXPECT_GIT_SHA`; `smoke-checks.sh` fails a packaged build whose sha is empty / `unknown` / `local`, or differs from the expected one. |
| L3 | `CARGO_ENCODED_RUSTFLAGS` (0x1f-joined): `$srcdir=/build`, `$CARGO_HOME=/cargo`. C/C++: `-ffile-prefix-map` for both paths, each skipped (with a message) only if it contains whitespace (cc-rs splits CFLAGS). The arch smoke builds in `BUILDDIR='/build/b d'` and fails if the binary has a `.rs` path under it or `/home/builder`. |
| L4 | `--printsrcinfo` with `FSYNC_LOCAL_SRC` set exits 1 (makepkg's `PRINTSRCINFO`, set before sourcing). Smoke: asserts the refusal, generates a release-mode `.SRCINFO` (dummy commit) and fails on `file://`. Git checksum: template comment + `updpkgsums` step (header, README, MVP phase 16). `glibc gcc-libs` were already in `depends` on main. |
| L8 | `apps/linux/scripts/lib/assets.list` (desktop file, 3 icons, LICENSE-MIT, LICENSE-APACHE; `<mode> <src> <dest>`). Read by PKGBUILD `package()`, `build-appimage.sh` (license lines), `smoke-checks.sh` (exists + mode). cargo-deb has no asset override (`--help` checked), so `scripts/check-assets.sh` compares the deb and rpm blocks of `app/Cargo.toml` with the list and runs first in `package-in-container.sh`. |
| Maintainer | PKGBUILD `# Maintainer: alex <danghuyhoa@gmail.com>`, deb `Maintainer:`, rpm `Vendor:` (cargo-generate-rpm 0.21.1, the latest, has no `packager` key; source checked). |
| License | `LICENSE-MIT`, `LICENSE-APACHE` (standard text + appendix, `Copyright (c) 2026 alex`). `license = "MIT OR Apache-2.0"` in both workspaces (all root crates inherit it). PKGBUILD `license=('MIT OR Apache-2.0')`; rpm License from Cargo; DEP-5 `packaging/deb/copyright` (header stanza `MIT or Apache-2.0`, one stanza each) shipped as a deb asset (cargo-deb then generates none). Both texts in `/usr/share/licenses/fsync-linux/` in every format, AppImage included. README License section. macOS/Windows metadata: no license named (grep), unchanged. |
| cargo-about | `private = { ignore = true }` still works: the repo crates are `publish = false`, the report built in the release container and in makepkg (`--fail`). |
| L5 | Deferred: item 7 + todo in `plans/261013-2029-fsync-native-mvp/phase-16-release-distribution.md`; ADR 014 notes. |
| Docs | ADR 014 decisions 17/18 updates, apps/linux README, codebase-summary, changelog 2026-11-11, roadmap. |

## Verification
- Root `cargo build`: ok (2m55s). `cargo metadata`: every crate `MIT OR Apache-2.0`.
- shellcheck (koalaman/shellcheck:stable, `-x -S warning`): check-assets.sh, smoke-install.sh, smoke-checks.sh, package-in-container.sh, lib/common.sh, build-appimage.sh clean; PKGBUILD clean with SC2034/SC2154/SC2164 excluded.
- Host makepkg: `FSYNC_LOCAL_SRC=... makepkg --printsrcinfo` refused (exit 1); release mode prints `license = MIT OR Apache-2.0`.
- `build-release.sh` at 209bccf (docs uncommitted, so `209bccf-dirty`): `check-assets: ok (6 listed files)`, build-info `{"features":[],"git_sha":"209bccf-dirty",...}`. Deb: `Maintainer: alex <...>`, DEP-5 copyright, both licenses. Rpm (`rpm -qip` in fedora): `License: MIT OR Apache-2.0`, `Vendor: alex <...>`.

| Smoke target | Result |
|---|---|
| ubuntu-2204 deb | PASS (17 PASS, 0 FAIL; git_sha, 6 assets + modes, DEP-5) |
| ubuntu-2404 deb | PASS (17/0) |
| debian-12 deb | PASS (17/0) |
| fedora rpm | PASS (16/0) |
| appimage (ubuntu:22.04) | PASS (13/0, both licenses in the AppImage) |
| appimage-fedora | PASS (13/0) |
| appimage-arch | PASS (13/0) |
| arch (run 1, at ebe332c) | FAIL on the new remap check only: `/home/builder/.cargo/registry/.../cxx-1.0.205/src/cxx.cc` (C++ `__FILE__`). Before that: .SRCINFO guard PASS, release .SRCINFO PASS, makepkg build 24 min + check (platform tests ok), `package()` build-info `git_sha":"ebe332c"` asserted. Fixed in a69e515 (CARGO_HOME also in `-ffile-prefix-map`). |
| arch (run 2, at a69e515) | PASS, container exit 0. .SRCINFO guard + release .SRCINFO PASS; BUILDDIR `/build/b d`; `package()` build-info `git_sha":"a69e515"`; "no build dir or builder home paths in the binary" PASS; `PASS namcap: no errors` (warnings only, as before: QML module `app.fsync`, ld-linux, implicit libstdc++/libgcc, `qt6-wayland` and `gcc-libs` "may not be needed"; no license warning); smoke-checks 16 PASS incl. git_sha, 6 assets + modes; ALL CHECKS PASSED. Package 13 MB, Qt 6.12.0, OpenSSL 4.1.0. |

Note: the attached `docker run` client of arch run 1 exited after about 30 min (likely the background shell limit) while the container kept running; the result was read from `docker logs -f` / `docker wait` of that container (mine, auto-removed).

## Unresolved questions
1. rpm `Packager` stays empty (cargo-generate-rpm cannot set it); the maintainer is in `Vendor`. OK, or post-process the rpm?
2. DEP-5 uses the short name `MIT` (Debian's preferred name for this text is `Expat`). Switch if lintian is ever run?
3. With whitespace in BUILDDIR the C/C++ `__FILE__` of files under `$srcdir` keeps the build path (cc-rs limit). Accept, or refuse such a BUILDDIR?
4. cargo-about logs `ERROR failed to parse license 'GPL-1.0'` (deprecated id in some crate's metadata) but `--fail` does not stop; pre-existing, not investigated.

## Fix round (review 8.5, code-reviewer-261111-0427-phase-04.md)
Commits 3f2c74e (code), 0c59985 (docs + review report).
- H1: `about.toml` clarify `libsqlite3-sys` = `MIT AND BSD-3-Clause` (LICENSE + sqlcipher/LICENSE, sha256-pinned). The generated report (release image, cargo-about 0.9.3) has the Zetetic LLC text (BSD-3 now 6 crates). macOS/Windows ship no third-party report at all: noted as MVP phase 16 item 8 (not built).
- M1: about.hbs triple-stash; the report has 0 HTML escapes. M2: `accepted` pruned to the 10 ids the graph uses. Negative check: dropping Zlib makes `cargo about generate --fail` exit 1. M3: MPL-2.0 crates.io source note (attohttpc 0.30.1).
- L1: check-assets compares the whole deb/rpm blocks (built files are exact expected lines). Mutation tests in a copy: an extra stage asset, a wrong copyright dest, a wrong binary mode and a relative dest all fail. L5: list lines are format-checked.
- L2: the arch smoke greps the guard message (`expect_refused`). L4: `--source`/`--allsource` (SOURCEONLY) are refused too (host makepkg checked).
- L3, decision: refuse whitespace in BUILDDIR/CARGO_HOME. Reason: cc-rs cannot remap C/C++ there, so "no build paths in the binary" could only be kept by refusing. The smoke checks the refusal (`--nobuild` with a space), builds in `/build/work` and greps the whole build dir and `/home/builder`.
- L6: the README recipe uses `fsync_git_sha` (keeps `-dirty`). L7: documented in the PKGBUILD header and README. L8: DEP-5 uses `Expat`; the smoke grep was updated. L10: ADR/changelog say "documented, deferred to phase 16". L9 accepted; rpm Packager empty + Vendor accepted. LGPL Qt source offer added to MVP phase 16.
- Smoke checks now also need the SQLCipher notice and no HTML escapes in the report.
- Verified: check-assets ok, shellcheck clean (scripts + PKGBUILD).
- NOT yet rerun at hand-back: build-release.sh (running detached, log /home/alex/.cache/fsync-p04-logs/build-release2.log), then the deb/rpm/appimage smokes and the arch smoke.
