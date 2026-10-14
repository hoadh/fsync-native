# Round 2 review: fix/review-followups (7e92f3d, b66f1d3)

Score: 9/10.

## Verification
- `cargo test -p fsync-core --no-fail-fast`: everything passes, including `service::stop_leak_tests::leaked_engine_handle_is_closed_and_lock_released_after_bounded_hold`.
- The only 2 failures are `file_transfer::fetch_200_mib_matches_hash_and_lands_in_dest` and `abort_mid_transfer_then_resume_fetches_only_the_rest`. Both fail with `ENOSPC` (os error 28). The volume has 257 MiB free (100% full), so this is environmental, not a regression.
- The coordinator reported 269/0, which matches the passing results here.
- `swift test` was not run. The disk is nearly full, and a build plus xcframework link would likely fail for the same reason. The Swift conclusions below come from code reading only.

## Round-1 findings
| Finding | Status |
|---|---|
| Medium: live leaked engine without the lock | Fixed. The timeout now calls `close()`, then releases the lock. The test asserts `is_closed()` is true before the lock is free. |
| Medium: ungraceful last-drop after timeout | Fixed. The node is closed through `&self`, so the later last-drop is harmless. |
| Medium: sync or hotkey starting after `prepareToQuit` | Fixed. `isQuitting` gates `clipSettings`, `phaseChanged(.ready)` and `startClipboard`. `prepareToQuit` now also stops `clipSync` and cancels the inbox cleanup. |
| Low: tautological constant test | Fixed. It is now `const _: () = assert!(...)`. |
| Low: no `stop()`-level leak test | Fixed. `stop_leak_tests.rs` uses a real service, a real node, and an injectable wait through `stop_with`. |
| Low: unconditional `dismissMessage()` | Fixed. Only `settingsOffMessage` is cleared. |
| Low: stale log line | Fixed. |
| Low: no positive hotkey assertion | Partly fixed. The test now asserts `isRegistered == true`, or `XCTSkip` if another app owns the shortcut. It is now conditional. |
| Low: roughly 1 s left for graceful shutdown | Accepted. The reason is documented at engine_release.rs:9-15. |

## Specific checks
1. `close()` idempotency and races (engine/mod.rs:88-99, net/node.rs:153-166).
   - `enforcer.abort`, `feeder.abort` and the `TaskGuard` abort are idempotent.
   - `router.shutdown` and `ep.close` are documented as safe to repeat. The test exercises close, then drop, then `start()` and `stop()` again.
   - A concurrent `close` (leaked handle) racing with `shutdown(self)` is not possible. `shutdown` needs a unique Arc, and `close` is only called when it is not unique.
   - After close, in-flight callers get a connection error. That is the intended behavior.
   - `Node::shutdown(self)` and `Engine::shutdown(self)` now delegate to `close`, so there is one code path.
2. Deadlock from inside the runtime: none.
   - `close` is a plain `async fn`. It is awaited inside the `tokio::spawn`ed holder, with no `block_on` and no lock held across the await.
   - `stop_with` holds `start_gate` only until the spawn, not during `close`. A restart (`start`) during the hold therefore fails fast on `lock_data_dir` ("already running") instead of blocking.
   - At runtime teardown the holder is aborted, so `close` is skipped. The process is exiting, so that is acceptable.
3. `isQuitting` reset on reinstall.
   - `applicationShouldTerminate` (AppDelegate.swift:28-35) always returns `.terminateLater` and then replies `true`. A quit cannot be cancelled in this app, so quit-cancel then sign-in cannot happen today.
   - Even if it could, sign-out and sign-in only change `phase`. They do not call `installClipboardModels`, so `isQuitting` would stay true and sync would stay off until a re-boot. That fails safe (nothing is resurrected), but it is a latent loss of function if cancellable quit is ever added.
   - `installClipboardModels` runs only from boot (`installAccountModels`), which creates new models, so the reset happens exactly when the old models are gone.
   - `removeClipboardModels` calls `prepareToQuit`, which sets `isQuitting = true` during every shutdown. The only reset is the next boot, which covers `startFresh`.
   - Stale in-flight loads from the old models are harmless. They call the old `ClipSettingsModel`'s `onChange`, which is `[weak self]`, and `clipSync` is the new instance.

## Remaining issues
- Low: stop_leak_tests.rs, the `!leaked.node().is_closed()` assertion after `stop_with(50ms, 600ms)`. It is timing-based and is flaky only if the thread stalls for more than about 550 ms. Consider a longer hold (for example 2 s) or an explicit "closed" signal. The lock-released poll already has a 10 s deadline.
- Low: the `isQuitting` doc comment says "until models are reinstalled". Document that quit is treated as final, so no cancel path is expected.
- Low: `testOnlyTheSettingsOffMessageIsClearedOnStart` accepts either "Some other notice" or the hotkey message. That is tolerant on purpose, but it cannot tell whether the hotkey report overwrote the notice, which is a pre-existing last-writer-wins `report`.
- Info: `Node::is_closed` is a new public API used only by tests. Consider `pub(crate)` or `#[cfg(test)]`, but check that it is not needed by an integration test in `tests/`.

No critical, high or medium issues remain.

**Status:** DONE_WITH_CONCERNS
**Summary:** All round-1 findings are resolved. `close()` is idempotent, deadlock-free and well tested. `isQuitting` blocks all start paths, and the reinstall reset cannot resurrect anything. Core tests pass except two `ENOSPC` failures caused by the full disk.
**Concerns/Blockers:** The Swift tests were not run because of the nearly full disk, so the Swift verdict comes from code reading. The `file_transfer` suite needs a free volume to confirm. Two Low items remain: one timing-dependent assertion, and a latent `isQuitting` stickiness if a cancellable quit is ever added.
