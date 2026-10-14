# Phase 11 macOS App Shell: Final Test Gates

Branch: macos/phase-11 | Env: macOS arm64 (Darwin 25), Xcode 27 / Swift 6, Rust 1.99, Docker running
Date: 2026-10-14. No source files edited.

## Results

| # | Gate | Result |
|---|------|--------|
| 1a | `swift build` (FsyncMac) | PASS, 0 warnings |
| 1b | `swift test -Xswiftc -strict-concurrency=complete` | PASS: 89 run, 3 skipped (integration, no `FSYNC_IT_API`), 0 failed, 0 warnings |
| 2 | `swift test` (FsyncCoreKit) | PASS: 8 FsyncCoreKitTests, 14 TrustResetPromptTests (listed separately), 0 failed, 0 warnings |
| 3 | `run-fsync-mac-integration.sh` | PASS: 3/3 integration tests passed (~15 s), `[mac-it] PASS` |
| 3a | Post-run cleanup | Clean: no `fsync-mac-it` container, no stray xctest/fsync-cli, 0 `app.fsync.mac.tests.*` Keychain items |
| 3b | `mystifying_euclid` | Untouched (still Up) |
| 4a | `cargo fmt --all --check` | PASS, exit 0 |
| 4b | `cargo test --workspace` | PASS: 253 passed, 0 failed, 1 ignored (matches baseline), 0 warnings |
| 5 | `build-fsync-mac-app.sh` | PASS, release build, ad-hoc signed |
| 5a | `codesign --verify --strict` | PASS: valid on disk, satisfies Designated Requirement |
| 5b | `plutil -lint` | PASS: Info.plist OK, all nested plists OK |
| 5c | Bundle removal | Done. `apps/macos/build/` removed (empty after bundle delete) |

## Failures

None.

## Notes

- Gate 1 `swift build` first waited on another SwiftPM process (PID 14866) holding `.build`. It then completed. No action taken.
- Gate 1 and 2 `EXIT=` values were lost to a pipe in my command. Pass status is taken from the `Executed N tests, with 0 failures` lines and a zero warning/error count in the saved logs.
- Logs kept in scratchpad: `fsyncmac-test.log`, `corekit-test.log`, `integration.log`, `cargo-test.log`, `bundle.log`.

## Critical Issues

None. All gates met 100%.

## Recommendations

- Gate 1 counts: 89 tests and 3 skipped match the expected numbers exactly.
- Consider making the `swift test` gate fail-fast on `pipefail` so exit codes are not lost in future runs.

## Unresolved Questions

- The 3 skipped tests run only under `run-fsync-mac-integration.sh`. Is a CI job wired to run that script on each PR, or is it manual-only?
- `apps/macos/build/` was removed as empty. Confirm it was not expected to persist between runs.

**Status:** DONE
**Summary:** All 5 gates pass at 100%. FsyncMac 89 tests (3 skipped, 0 failed, 0 warnings), CoreKit 8+14 pass, integration PASS with clean teardown, Rust 253 passed / 1 ignored (baseline), app bundle signed and plist-valid, bundle removed.
**Concerns/Blockers:** None. Minor: exit codes for two swift test runs were lost to a pipe; pass status verified from logs instead.
