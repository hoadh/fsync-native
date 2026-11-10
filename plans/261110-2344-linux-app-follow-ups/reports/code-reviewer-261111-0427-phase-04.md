# Code review: phase 04 packaging hardening (32aecf8..f9ffbae)

Score **8.5/10**. Critical 0, high 1, medium 3, low 10. NOT approved (bar 9.5). Persisted by the controller from the reviewer's hand-back.

## Verified
- `cargo metadata` over both workspaces: no crate declares a GPL license, and every repo crate is `MIT OR Apache-2.0`. Nit: `spikes/*` crates have no `license` field.
- `check-assets.sh` was mutation-tested: it catches a missing, extra or wrong-mode asset (but see L1).
- `.SRCINFO` guard: refused with `FSYNC_LOCAL_SRC` set. In release mode the output is clean.
- deb, rpm, AppImage and Arch artifacts carry the license files, maintainer and sha as claimed.
- The AppImage ships Qt as replaceable `.so` files, so the LGPL-3 condition is met.
- LICENSE texts: MIT matches SPDX; Apache is standard, with the appendix filled in.
- PKGBUILD:
  - the sha logic is correct in both modes;
  - `CARGO_ENCODED_RUSTFLAGS` uses the `\x1f` separator correctly;
  - it does not conflict with the release guard.

## "GPL-1.0"
The text is `openssl-src/.../external/perl/Text-Template-1.56/LICENSE` (the Perl terms). It is a build-time Perl module that is never linked or shipped, and cargo-about only logs a diagnostic for it. No dependency blocks `MIT OR Apache-2.0`:
- MPL-2.0 `attohttpc` is file-level;
- `r-efi` has a permissive option;
- Qt is dynamically linked.

## High
- **H1:** SQLCipher's BSD-3 notice (Zetetic LLC) is missing from THIRD-PARTY-LICENSES in every format. `libsqlite3-sys` declares MIT but statically compiles `sqlcipher/sqlite3.c`, and BSD-3 requires the notice to be reproduced in binary distributions. Fix: an `about.toml` `[libsqlite3-sys.clarify]` entry (`MIT AND BSD-3-Clause`, files `LICENSE` and `sqlcipher/LICENSE`) or a template append. Optional: quote OpenSSL's own `LICENSE.txt`.

## Medium
- **M1:** `about.hbs:9,13,16` HTML-escapes the plain-text report (`&quot;`, `&#x3D;`). Use `{{{ }}}`.
- **M2:** the `about.toml` accepted list is wider than needed (`LGPL-2.1-or-later`, `OpenSSL`, `BSL-1.0`, `CC0-1.0`), so a future LGPL-only crate would pass `--fail`. Prune the list, and run one negative check to prove that `--fail` fails.
- **M3:** MPL-2.0 `attohttpc` needs a source-availability note (crate and version on crates.io).

## Low
- **L1:** `check-assets.sh:20` skip filter is unanchored. An extra asset under `dist/stage/` passes, and so does a wrong copyright dest. The binary and THIRD-PARTY lines are never checked.
- **L2:** `smoke-install.sh:117-120` treats any non-zero exit as "refused". Grep for the message instead.
- **L3:** the arch smoke never exercises the C/C++ `$srcdir` remap, because the space BUILDDIR always skips it. Refuse a whitespace BUILDDIR, or assert on a space-free path.
- **L4:** the `.SRCINFO` guard does not cover `makepkg --source/--allsource` (`SOURCEONLY`).
- **L5:** asset-list lines are not format-checked: a dest without a leading `/` would install relative. Validate `^[0-7]{3} \S+ /\S+$`.
- **L6:** the README tarball recipe drops `-dirty`. Point it at `fsync_git_sha`.
- **L7:** PKGBUILD still honours `~/.cargo/config.toml` (profile, rustc-wrapper, `[env]`). This is pre-existing; document it or use a clean `CARGO_HOME`.
- **L8:** DEP-5 `MIT` should be `Expat`. Adjust the smoke grep to match.
- **L9:** rpm has no `%license`; THIRD-PARTY is marked `doc`. Acceptable.
- **L10:** ADR 014 and the changelog say the git checksum is "done". Reword to "documented, deferred to phase 16".

## Unresolved questions
1. Fix H1 in this phase (in scope)?
2. Is `--fail` behaviour on an unaccepted license confirmed?
3. Copyright holder "alex": intended?
4. rpm `Packager` empty with the maintainer in `Vendor`: acceptable?
5. Source offer for the AppImage's LGPL Qt: is that phase 16?
