# Code review: phase 01 (Linux app spike + scaffold)

Scope: `apps/linux/` (Cargo.toml, .gitignore, README, platform/, app/{build.rs,src,cpp,qml,.qmlls.ini}, scripts/, Dockerfiles), `docs/adr/014-linux-desktop-app.md`. About 450 LOC excl. lock. All source files are under 200 lines.
Verified: cxx-qt-build/lib 0.10.0 source, the lock drift script run (rc=0, 291 crates), `cargo tree` with build edges, GNU `realpath` behaviour, and that the root workspace is Qt-free (0 cxx-qt entries in the root lock).

## Score: 7.5/10
The code is small and tidy and does what the spec asks. The smoke design is sound. No case lets `--smoke` pass when it should fail on load. Two issues can break the build or weaken a guarantee (.qmlls.ini, lock drift skips build deps). The rest is robustness.

## Critical
None.

## High
1. **`app/.qmlls.ini`: committed with a host path, or a clean-checkout container build panics.** cxx-qt-build 0.10 (lib.rs:1011-1039) creates `$CARGO_MANIFEST_DIR/.qmlls.ini` with `create_new`. It ignores `AlreadyExists` and panics on any other error. The file is not gitignored and contains `/home/alex/Work/...`.
   - If it is committed, every clone gets your absolute path.
   - If it is ignored (the correct choice), a fresh clone has no file. `build-in-container.sh` mounts the repo `:ro`, so the create fails with EROFS and the build panics with "Could not create qmlls.ini file".
   - Container builds pass today only because a host build created the file first.
   - Fix: add `/app/.qmlls.ini` to `.gitignore`. In `build-in-container.sh`, before `docker run`, create it on the host if it is missing, with the same content the host build would write (`buildDir="$repo/apps/linux/target/cxxqt/qml_modules"`, `no-cmake-calls=true`).
2. **The lock drift check skips build deps, so the vendored OpenSSL source is not covered.** `-e normal` drops `openssl-src` (a build-dep of openssl-sys, i.e. the vendored OpenSSL itself) and `cc`. The script and ADR 014 #2 say the app "must link exactly what root tests exercise". Both locks agree today (`300.6.1+3.6.3`), but nothing guards it.
   - I checked `-e normal,build`: the only difference is `jobserver v0.1.35`, from cxx-qt's `cc/parallel`.
   - Fix: use `-e normal,build` and filter `jobserver` (`grep -v '^jobserver '`). Update the comment and the ADR wording.

## Medium (warnings)
3. **QML warnings during teardown are not counted.** `run_gui` reads `qml_warnings()` after `exec()` but before `engine` and `app` are dropped (they drop on return). Warnings from bindings during destruction ("Cannot read property of null"), a common class of QML bug, therefore never fail the smoke. Fix: `drop(engine);` (then `drop(app)`) before the count check.
4. **`std::process::exit(1)` inside the `objectCreated` callback.** It runs inside `engine.load()`. libc `exit` then runs static and Q_GLOBAL_STATIC destructors while `QGuiApplication` and the engine are still alive higher up the stack, which can crash on the way out. The exit code is still non-zero, so this is not a false pass, but the result is noisy.
   - Loading from qrc is synchronous, so a cleaner approach is possible: set a `Rc<Cell<bool>>` flag in the closure, then `if failed { return 1 }` right after `load()`, before `exec()`.
   - This works on Qt 6.2: cxx-qt-lib cfg-gates `object_creation_failed` to Qt ≥ 6.4, and `objectCreated(nullptr)` is emitted on all Qt 6 versions, so the current choice is correct.
5. **`run-in-container.sh` with a wrong binary path creates a root-owned directory on the host.** GNU `realpath` succeeds for a missing last component (verified rc=0). `docker -v` then creates the missing host path as a root-owned directory, e.g. `target-ubuntu-2204/release/fsync-linux/`, which later blocks cargo from writing the binary there. Fix: `[[ -f "$2" ]] || die`, or use `--mount type=bind,src=...,dst=...,readonly`, which errors on a missing source.
6. **The smoke quit trigger depends on the spike `HelloObject`.** `Qt.callLater(Qt.quit)` only fires from `HelloObject.onStatusChanged`. When phase 02 deletes HelloObject without moving the trigger, every smoke run ends through the 5 s timer with exit 2. Note this in phase 02.
   - ADR 014 #7 says "loads the QML root, quits". It should also mention the worker round trip and the exit-2 timeout.
7. **The toolchain is pinned twice.** The Dockerfiles have `ARG RUST_TOOLCHAIN=1.99.0` and `rust-toolchain.toml` has its own pin. If the toml is bumped, the rustup proxy in the container tries to install the new toolchain into `/opt/rust`. That directory is not writable for `--user uid`, so the build fails with a confusing permission error.
   - Fix: in both scripts, pass `--build-arg RUST_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$repo/rust-toolchain.toml")"`.
8. **`check-lock-drift.sh` fails silently.** `2>/dev/null` plus `set -e`/`pipefail` on `root=$(core_tree ...)` makes the script exit with cargo's code and no output. The "cargo tree failed" branch is never reached. This is not a false pass. Fix: drop `2>/dev/null`, or use `root=$(core_tree ...) || { echo ...; exit 2; }`.

## Low
- **Rustup install via `curl | sh`.** Acceptable for a local build image: HTTPS, official host, `-f`. Add `--proto '=https' --tlsv1.2`, as rustup recommends. Before phase 05 builds release artifacts in these images, switch to a pinned `rustup-init` (versioned archive URL + `.sha256` check).
- **`smoke_log.h` is not tracked for rebuilds.** `cpp_file` only emits rerun-if-changed for the `.cpp` file, so header edits don't trigger a rebuild. Add `println!("cargo::rerun-if-changed=cpp/smoke_log.h")` to build.rs.
- **`is_qml_message` filter.** It fails safe: it may false-fail when a newer Qt emits `qt.qml.*` deprecation warnings. That direction is acceptable for a gate, but document it.
- **ADR #4.** "Ubuntu 22.04 / Debian 12 (glibc 2.35, Qt 6.2.4)" describes only Ubuntu. Debian 12 is glibc 2.36 / Qt 6.4.2.
- **ADR #5.** "accepted (or safely recompiled)" is ambiguous. The test only proves that one trivial QML file loads. Newer Qt most likely discards the 6.2 cache units and compiles the QML source from qrc (AOT benefit lost on newer Qt, correctness kept). Say that, and keep the fedora/arch runs as the gate in phase 05 once the QML grows.
- **Duplicate Dockerfiles.** `ubuntu-2204` and `debian-12` differ only in `FROM`. One file with `ARG BASE` would do (DRY).
- **check-lock-drift.sh sed.** `sed 's/ (\*)$//'` is dead code after `s/ (.*$//`.
- **Unpinned `:latest` images.** Fedora/arch use `:latest`, which is intended (newest Qt). Record the image digest in the ADR results for traceability.
- **Unused workspace deps.** `tokio`, `tracing`, `thiserror`, `anyhow` are declared but unused. This is per spec, so fine.

## Focus answers
- **`--smoke` false-pass paths:** no load false-pass.
  - A null root makes the process exit 1.
  - Binding and console errors are counted through QQmlEngine's `dumpwarning`, which passes an explicit file URL, so `.qml` matches even with release Qt and no log context.
  - Timer → exit 2.
  - Gap: teardown warnings (#3).
  - Qt 6.2 has no `objectCreationFailed`, which is handled correctly.
- **Message handler thread safety:** OK. The atomic counter is safe. `g_previous` is written once, before QGuiApplication and core/tokio threads exist, so later handler calls on other threads see it (happens-before via thread start). `qInstallMessageHandler` returns `qDefaultMessageHandler` rather than null, so output keeps chaining.
- **cxx bridge `smoke_log`:** correct.
  - `unsafe extern "C++"` is required in edition 2024.
  - Safe fns without pointers are fine.
  - `i32`↔`int` matches.
  - The include path resolves through the crate include root.
  - The plain `cxx::bridge` in `.files()` is accepted by cxx-qt-build.
  - Keeping it in `bridge/` satisfies the one-directory rule, and ADR #3 is accurate (lib.rs:854-880).
- **Scripts:**
  - uid handling, ro repo, host-owned caches and the nested target mount over the ro mount are all fine (mkdir runs first).
  - `bash -euc` propagates exit codes.
  - Problems: #1, #5, #7, #8. Rootless docker/podman would map `--user` to a subuid (informational).

## Positive
- Separate workspace keeps the root Qt-free (verified).
- Exact CXX-Qt pins.
- Toolchain under `/opt`, readable by any uid.
- Host-owned caches, read-only repo mount.
- `--locked` everywhere.
- Smoke covers the worker→Qt `queue` round trip, which is the API phase 02 depends on.
- Core smoke covers the vendored SQLCipher/OpenSSL.
- The ADR results table is concrete (glibc max, NEEDED list, Qt versions).

## Recommended actions (priority)
1. Gitignore `.qmlls.ini` and pre-create it in `build-in-container.sh` (#1).
2. Lock drift: `-e normal,build` minus jobserver; remove the silent failure (#2, #8).
3. Drop the engine before counting warnings; replace `process::exit` in the callback with a flag (#3, #4).
4. Check that the binary exists in `run-in-container.sh` (#5); derive the toolchain build-arg from rust-toolchain.toml (#7).
5. ADR wording (#4 floor, #5 cache, #7 smoke); add a phase 02 note on the smoke quit trigger (#6).

## Unresolved questions
- Is `app/.qmlls.ini` meant to be committed? I assume not.
- Phase 05: will release artifacts be built in these same images? If yes, the rustup checksum pin becomes required.
