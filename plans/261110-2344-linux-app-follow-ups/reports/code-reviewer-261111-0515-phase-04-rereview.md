# Code re-review: phase 04 fix round (3f2c74e, 0c59985, 04b52b8)

Score **9.5/10**, APPROVED. The condition (release build and smokes pass) is met. Persisted by the controller from the reviewer's hand-back.

## Verified
- **H1:** the `about.toml` clarify sha256 values match the registry files. The smoke fails if a package's report lacks the Zetetic notice.
- **M1:** `about.hbs` uses triple braces.
- **M2:** the accepted list is pruned to 10 licenses, and every remaining one is needed; 425 crates checked, 0 unsatisfied. The negative `--fail` check rests on the implementer's report only: cargo-about is not installed on the host.
- **M3:** the MPL source note is rendered.
- **L1/L5:** the rewritten `check-assets.sh` caught all 15 mutations (rc=1); baseline rc=0.
- **L2:** the refusal check matches the guard's message.
- **L3:** a whitespace `BUILDDIR`/`CARGO_HOME` is refused. The remap flags are correct, and `prepare` runs before the `--nobuild` exit.
- **L4:** `--printsrcinfo`, `--source` and `--allsource` are all refused with `FSYNC_LOCAL_SRC` set.
- **L6, L7, L8 (Expat), L10:** done.

## Remaining (nits)
- The deb copyright `Comment:` line is about 117 characters.
- GCC splits `-ffile-prefix-map` at the first `=`. A build path containing `=` would break only the C/C++ remap; the guard could also reject `=`.
- The check-assets format regex accepts `../`-style sources. This is not a real threat.
- rpm `Packager` is empty and the maintainer is in `Vendor`; rpm has no `%license` (accepted).

## Smoke results (controller, at 04b52b8)
- `build-release.sh`: rc=0.
- ubuntu-2204 deb, fedora rpm, appimage: ALL CHECKS PASSED. Each shows `git_sha is 04b52b8`, `report credits SQLCipher (Zetetic LLC)` and `report is plain text`.
- arch PKGBUILD: rc=0, ALL CHECKS PASSED, `PASS namcap: no errors`. The guards refuse `--printsrcinfo`, `--source` and the whitespace `BUILDDIR`. `no build dir or builder home paths in the binary`.
