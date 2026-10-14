# Review: fix/review-followups (7817300, caed6cc, b00d756)

Score: 7.5/10. Rust: `cargo test -p fsync-core --lib engine_release` passes 5/5. Swift tests were not run.

## Q1. Lock released after 60 s, leaked engine still alive
What the lock protects. `lock_data_dir` says it prevents two engines splitting incoming clips and fighting over history.
- `history` and `store` are `Arc`s owned by `Inner` and shared by every engine (lifecycle.rs:28-30). A leaked engine and a new one write through the same db handle and blob store.
- I found no second sqlite handle, so I do not expect db or blob corruption. The risk is functional.
  - The leaked engine's iroh node is still bound and still has ALPN handlers.
  - Peers can connect to it, and it delivers into the same sink and history as the new engine.
  - Result: duplicated or misrouted incoming clips, plus ghost presence. This is the failure the lock was meant to prevent.
- This is acceptable compared with the original deadlock, where login could never restart. The bound is a reasonable fail-open.

Does a leak happen in practice?
- Leaked `Arc<Engine>` handles come from in-flight FFI calls. `self.engine()?` clones the Arc, and the clone is held across `.await` in `publish_clip` (clips.rs:89, 113) and `send_files`. Pairing and transfers do the same.
- Aborting tasks does not drop those caller futures. Swift runs calls via `EngineCall.run` off the main thread, and `engine.stop()` runs concurrently.
- A push of a large file to a slow peer can plausibly exceed 60 s. That is a realistic leak, not just a bug scenario.
- I did not inspect the Windows host. `apps/windows` exists, but I did not trace its stop path.

Better bound (Medium). Release the lock only after the leaked engine's tasks are stopped. Add `Engine::close(&self)` that aborts the enforcer and feeder and shuts down the router and endpoint through a shared reference.
- The 60 s timeout would then call `close`, so the lock is released with nothing live behind it.
- In-flight callers would fail with a connection error instead of continuing against a zombie node.
- This needs `Node::shutdown(&self)` or interior mutability, because `Node::shutdown(self)` consumes (node.rs:153).

Related regression (Medium), engine_release.rs:60-68. In the `Err(_leaked)` arm the holder drops its Arc.
- The last handle to drop later drops `Engine` without a graceful `shutdown().await`.
- Previously the holder waited forever, so shutdown was always graceful. The "release anyway" path loses that.
- It is probably tolerable if the router and endpoint have Drop semantics, but I did not verify that.
- Fix: keep polling after the lock is released and shut down when finally unique, or use `close(&self)` above.

At teardown the spawned holder is killed with the runtime (mod.rs:111, `rt.shutdown_timeout`). That is fine, because the process is exiting.

## Q2. M2 correctness
- The change is correct and does not affect logout's inline revoke. `logout()` (login.rs:64) calls `sign_out_locally` plus `revoke_or_queue` (revoke.rs:32, `INLINE_BUDGET` 3 s). It never calls `stop()`.
- The inline wait and the revoke budget are independent code paths. The revoke future is held by the logout caller, not by `stop()`.
- The `TEARDOWN_STOP_TIMEOUT` timeout wraps only `inner.stop()`, in `teardown` on drop.
- The old inline wait was 5 s (50 x 100 ms), longer than the 3 s teardown cap. So teardown cut it off and the lingering handoff never ran on drop. The new 2 s wait fixes that real bug.
- Remaining nit (Low), lifecycle.rs:87-94. If `wait_unique` succeeds near 2 s, only about 1 s is left for `e.shutdown().await` (router and endpoint close). The teardown timeout can cancel it, drop the engine and release the lock. That is better than before but still ungraceful in the worst case.
- Blocking FFI `stop()` now blocks the caller for at most 2 s instead of 5 s. Good for UI threads.

## Q3. macOS double start, sign-out, quit
- Double start: not possible on the main actor. `ClipboardSync.start()` sets `running = true` synchronously, and `clipSettings(changed:)` is gated on `isRunning == false`. Overlapping loads (ready, signedOut, ready) are safe because the second sees `running`.
- After sign-out: protected. `phaseChanged(.signedOut)` calls `stop()`, and `clipSettings` has `guard phase == .ready`. A stale in-flight load cannot start sync.
- After quit (Low-Medium), EngineBridge+Clipboard.swift:30-37.
  - `prepareToQuit()` clears the pasteboard and unregisters the hotkey. It neither stops `clipSync` nor sets a quit flag. `phase` stays `.ready`.
  - If the first load failed (`isRunning == false`) and a later load or retry completes after `prepareToQuit`, `startClipboard()` runs. That starts the watcher and re-registers the hotkey during quit.
  - The window runs until `removeClipboardModels` in `shutdown()`, which first awaits `sessionClear`.
  - The old code had a similar window, but this change widens it to any later load.
  - Fix: add an `isQuitting` flag, or stop `clipSync` in `prepareToQuit`, and check it in `clipSettings`.
- `startClipboard()` calls `dismissMessage()` unconditionally (Low). That can clear an unrelated message, for example a hotkey conflict or an earlier error. Only dismiss the settings-read message.
- `historyModel.reload()` is now an unstructured Task and is not cancelled on stop (Low, harmless).
- File sizes are fine: Clipboard.swift 98 lines, ClipSettingsModel 85, lifecycle.rs 127, engine_release.rs 145 (including tests).

## Q4. Test quality
- Rust tests are real, using real flock files, and cover the hold, release and shutdown paths of the helper. Gaps:
  - Nothing tests `Inner::stop()` end to end with a leaked `Arc<Engine>`, so the wiring in lifecycle.rs:85-106 is untested.
  - `inline_wait_fits_inside_teardown_timeout` (engine_release.rs:~110) asserts on constants. It is tautological and may trip clippy `assertions_on_constants`. Make it a `const _: () = assert!(...)`.
  - The tests do not check that the lock stays held until just before the deadline. Fine, but the only timing check is a loose `elapsed < 5 s`.
- Swift tests use a real engine plus one seam (`readSettings`), which is legitimate.
  - Test 1 never asserts `hotkey.isRegistered == true` after the recovery. It asserts false before, but the hotkey start path is only implicitly covered.
  - Test 1 does not cover double start, sign-out then late load, or the quit race.
  - Test 2 only checks that sync is off in `signedOut`. It is weak because it does not prove `onChange` fired.
  - Using `eventually` for the message is fine.

## Issues by severity
- Critical: none.
- High: none.
- Medium
  - engine_release.rs:55-68 and lifecycle.rs:98-105: after 60 s a live leaked engine runs without the lock, and a later last-drop is ungraceful. Fix with `Engine::close(&self)` on timeout (see Q1).
  - EngineBridge+Clipboard.swift:30-37 and :85: sync and hotkey can start after `prepareToQuit` (see Q3).
- Low
  - engine_release.rs:~110: tautological constant test.
  - No `stop()`-level test of the leak path.
  - EngineBridge+Clipboard.swift:62: unconditional `dismissMessage()`.
  - lifecycle.rs:87: about 1 s left for graceful shutdown in the worst case.
  - Test 1 lacks a positive hotkey assertion.
  - The log line "shutting down once released" is now inaccurate. The holder may only release the lock.

## Positives
- The helper is extracted and generic over `T`, so it is testable without the engine.
- The constants are derived from one source, with a documented rationale for the margin.
- The macOS start logic is a single idempotent entry point gated on `isRunning` and `phase`.

## Unresolved
- Does Router/Endpoint Drop tear down cleanly when the last Arc drops without `shutdown()`?
- How does the Windows host call stop, and does it hold long-running sends during logout?

**Status:** DONE_WITH_CONCERNS
**Summary:** The fixes work and the Rust tests pass. M2 is correct and does not touch logout's revoke, which is a separate path. The macOS double-start and sign-out races are safe. Score 7.5/10.
**Concerns/Blockers:** After 60 s a live leaked engine runs without the lock, and its eventual drop is ungraceful, so add `Engine::close(&self)`. A late settings load can start sync and the hotkey after `prepareToQuit`. The stop-level leak path is untested. Swift tests were not run.
