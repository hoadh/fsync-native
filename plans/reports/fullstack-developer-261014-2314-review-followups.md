# Review follow-ups: core M1/M2, macOS M-new-1

Branch `fix/review-followups` (from ee9e2c8).

## M1: bounded lingering engine lock (core)
- New `core/fsync-core/src/service/engine_release.rs`: `wait_unique` (bounded poll for sole Arc owner), `hold_lock_until_released` (generic, injectable poll/max_hold/shutdown), const `LINGERING_LOCK_MAX_HOLD = 60s`.
- `lifecycle.rs` stop(): lingering path now spawns `hold_lock_until_released`; after 60 s releases `engine.lock`, drops its handle, logs warn "engine handle leaked".
- Tests (short injected durations): leaked handle frees real flock after bounded hold and never calls shutdown; released handle shuts down then frees lock; `wait_unique` both paths.

## M2: stop wait vs teardown timeout (core)
- `TEARDOWN_STOP_TIMEOUT = 3s` (used by `teardown` in mod.rs) is now the single source; `INLINE_RELEASE_WAIT = TEARDOWN_STOP_TIMEOUT - 1s margin` (2 s, was 50x100ms = 5 s).
- Why: teardown cancels `stop()` at its timeout, dropping the engine and lock held by that future; the inline wait must end first so a lingering engine is handed to the background holder.
- Test: `inline_wait_fits_inside_teardown_timeout`.

## macOS M-new-1
- `EngineBridge+Clipboard.swift`: start sequence moved to `startClipboard()`, triggered from `clipSettings(changed:)` (every successful load/edit) when phase == .ready and `clipSync` not running. Failed initial load only reports; any later successful load starts sync + hotkey + inbox cleanup + history reload and clears the stale "sync is off" message. No double start (isRunning guard).
- `ClipSettingsModel.readSettings`: internal test seam for load.
- New `ClipboardStartAfterFailedLoadTests.swift` (2 tests): failed load then success starts sync; success outside .ready does not start.

## Gates
- cargo fmt --check: pass. clippy workspace all-targets -D warnings: pass.
- cargo test --workspace: 269 passed, 0 failed, 2 ignored (65 suites; Postgres tests included).
- xcframework rebuilt via scripts/build-xcframework.sh (gitignored).
- FsyncCoreKit swift test: 8, 0 failures. FsyncMac swift test: 190 (12 skipped integration), 0 failures.
- All changed files < 200 lines. FFI surface, windows/backend/deploy/docs untouched.

## Notes
- After the 60 s release a leaked engine keeps running without the lock (per spec); a new engine could overlap with it.
- No Rust test drives a real Engine through the 60 s path (generic helper tested instead).
- Hotkey registration not asserted in Swift test (depends on whether another app owns the shortcut).
- Shell used `export PATH=$HOME/.cargo/bin` instead of `source ~/.cargo/env` (tool blocked `source`).
