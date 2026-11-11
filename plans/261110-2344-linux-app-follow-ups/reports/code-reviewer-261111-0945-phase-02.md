# Code review: phase 02 (offline-publish lows + FFI contract 3)

Branch `fix/offline-publish-lows`: commits 11701e8, 17d8adf and c10cd4a on main 381de6e. 23 files, +391/-116.

**Score: 8.5/10.** Critical 0, high 0, medium 2, low 5. **Not approved yet** (the bar is 9.5). Both mediums are small fixes. M1 is one reordered line, already verified here.

## Scope and method
- I read every changed file and checked the implementer report's claims myself.
- Builds ran only from a `git archive` copy (`~/.cache/fsync-review02-src`, separate target and tmp dirs). After the mutations I diffed the copy against the branch and it was clean again.
- I did not compile Swift or C#. I read the code and regenerated the bindings.

## Medium

### M1: a send's result is reported before its lane is freed, so a live peer can show as `pending` (regression)
`core/fsync-core/src/clip/send.rs:195-196`: `run_lane` calls `finish_send` first. That runs `tx.send(r)` at :212 and wakes the publish. Only then does it call `lane.next()`, which clears `running`.

The old code was the other way round. `_turn` was dropped at the end of the match arm, before `tx.send`, so the lane was free before the result went out.

**Failure scenario:**
1. Publish N to a live peer returns once that peer's result arrives.
2. The app publishes N+1 right away.
3. If the runner thread is preempted between `tx.send` and `lane.next()`, `submit` still finds `running == true` and returns `Wait`. The publish then gets `busy`, so the live peer is reported `pending` at once, with no wait.
4. This cascades. The quick "busy" publish returns at once, so the next publish also finds the lane busy, now with N+1's attempt.

Effects:
- The UI says "not confirmed" for a peer that is online: macOS hint, Windows status, and the Linux static line.
- The clip is still delivered.
- `offline_peer_never_holds_up_a_publish` (`tests/clip_offline_peer_publish.rs:88`) can flake under CI load.

**Evidence:**
- A 2 ms pause injected between the two calls makes `offline_peer_never_holds_up_a_publish` fail at :88.
- A probe of 30 back-to-back publishes to a live peer reported every publish after the first as `pending`.
- Without the pause, the probe saw 0 of 300 on an idle host. The window is real but narrow.

**Fix (verified):** take the next clip before reporting.
```rust
let r = self.send_one(&peer, &j.frame).await;
job = lane.next();               // free the lane (or take the next clip) first
self.finish_send(&peer, j, r);
```
With this order, and the same 2 ms pause placed after `finish_send`, the probe shows 0 of 30 and `clip_offline_peer_publish` passes 6/6. A regression test could pause in a `test-hooks` hook at that point.

### M2: the new startup checks cannot fire for the v2-to-v3 mismatch, and the app crashes instead
The new doc comment on `contract_version` changed its UniFFI checksum from 18762 to 64396. UniFFI's checksum covers doc comments: the docstring is the only metadata change, and the regenerated checksum differs. Both bindings check every checksum on the first FFI call, before `contractVersion()` returns.

- **Swift:** `contractVersion()` → `rustCall` → `makeRustCall` → `uniffiEnsureFsyncFfiInitialized()` (`fsync_ffi.swift:284`), which ends in `fatalError("UniFFI API checksum mismatch...")` (:4414).
  - The guard at `EngineBridge+Boot.swift:57-63` is never reached.
  - `resumePendingRevoke()` may make the first FFI call earlier, in a detached task (Boot.swift:17-20), and crash there.
- **C#:**
  - The static constructor of `_UniFFILib` (`fsync_ffi.cs:885-887`) throws `UniffiContractChecksumException`. .NET wraps it in a `TypeInitializationException`.
  - That type is not in the catch filter at `App.xaml.cs:73`, so the exception escapes `async void OnLaunched` and the process ends.
  - Pre-existing and likely, but not verified on Windows: a missing or wrong-architecture DLL also fails inside that static constructor (`uniffiCheckContractApiVersion` P/Invoke). So `DllNotFoundException` and `BadImageFormatException` probably arrive wrapped too, and the filter misses them.

The result: an old binding against the new library (or the reverse) now fails at startup rather than at the first `publish_clip`, so L-new-1 is fixed in substance. But it fails with a crash, not the "could not load its engine" message. Two lines in `docs/ffi-contract.md` are therefore wrong:
- :10 says the apps "show a fatal ... message on a mismatch".
- :229 says "the apps' startup check catches the mismatch".

`project-changelog.md` is correct (it mentions the checksum failure). The implementer report's claim that the macOS check covers this mismatch does not hold.

Every future bump that edits the doc comment repeats this. Fix options (pick one or more):
- (a) Keep the `contract_version` doc comment stable. Move the version history into a `//` comment in `lib.rs`. Then `contract_version`'s checksum no longer changes with a bump, and a record-only change (like `pending`) reaches the friendly check. Restoring the exact v2 text brings back 18762, so v2 bindings would also reach it.
- (b) Windows: add `TypeInitializationException` (and `UniffiException`) to the filter, and show `e.InnerException?.Message ?? e.Message`.
- (c) At minimum, correct ffi-contract :10 and :229: on a checksum mismatch, macOS hits `fatalError` and Windows hits an unhandled `TypeInitializationException`.

## Low
- **L1:** `peer_send_lanes.rs:112-120` with `send.rs:191-198`: if `send_one` panics, the dropped `LaneRunner` also drops the waiting clip. That clip gets no unoffer and its `rx` gets "no answer". Before, a panic lost only its own clip, and the queued clip's task still ran. This only happens on a bug path; `log_panic` warns, and the offer expires in 10 min to 1 h. Document it next to `log_panic`, or accept it.
- **L2:** No test covers the superseded path's offer withdrawal (`send.rs:180-185`, `unoffer` through `finish_send`). This was already true before. The `"superseded"` text is only checked by eye; grep finds no test that references either. A mutation that drops the unoffer would pass.
- **L3:** The macOS check has no test of its own. `EngineBridge.expectedContract` and `FsyncCoreKitTests.swift:53` hard-code 3 separately. Something like `XCTAssertEqual(EngineBridge.expectedContract, contractVersion())` in FsyncMacCoreTests would tie the two together.
- **L4:** `PublishInFlight` (`send.rs:66`, :240-258) also counts pushes (`send_files` goes through `Shared::publish`). A push that overlaps a clipboard publish logs "overlapping clip publishes" even though pushes bypass the lanes. This is debug-level noise only.
- **L5:** Docs, cosmetic:
  - `ffi-contract.md:184`: the heading "Additive since v2 (no version bump)" still holds the `Published.pending` entry, which now says it bumped the version.
  - `ffi-contract.md:154`: the rewrapped line is longer than the file's wrap width.
  - The pre-existing "5 s per-peer timeout" wording from the previous review (L6) is unchanged.

## Lane state machine: verified correct (except M1)
- **No lost wakeup:** `submit` and `next` check and change `running`/`waiting` under the same lane mutex, and the runner re-checks after every attempt.
- **No stranded clip:**
  - When the runner ends, `next()` sets idle only if nothing waits, in the same critical section.
  - Abort, a refused `spawn` (after close), and panics all go through `Drop` (`done == false`), which frees the lane and drops the waiting job, so its `rx` reports Closed.
  - `abort` only takes effect at an await point. `finish_send` and `next()` are synchronous, so a lane is never left half-updated.
- **Close:**
  - After `background.close()`, a new submit to a busy lane parks its job until the aborted runner drops it. A submit to an idle lane spawns into a closed set, and the dropped future frees the lane.
  - In both cases the publish reports `failed` (Closed), as before.
- **Locks:** none is held across an await. The map guard is a temporary, and the lane guard is taken in sync sections only. `held()` takes the map lock and then the lane lock, but only in tests. No lock-order risk.
- **Latest-wins:** preserved. The running attempt keeps its own clip, and the waiting slot is replaced. A mutation that keeps the older waiting clip fails both `slow_live_peer_gets_the_newest_clip_never_an_older_one_after_it` and the unit test `a_newer_clip_replaces...`.
- **Superseded outcome:** same `Net("superseded by a newer clip")` plus unoffer, but it now resolves at once on replacement rather than when the running attempt ends. With `wait_all` that is a faster `failed`; otherwise nothing waits on it.
- **`Published` accounting:** unchanged. The publish loop is untouched apart from the hook.
- **`busy`:** now true from `submit`, earlier than the old `try_lock`. For serialized publishes this is equivalent, once M1 is fixed.
- **Pushes:** still bypass the lane (`send.rs:168-174`).

## Tests
- **The mutation claim holds.** I restored main's `peer_send_lanes.rs` and `send.rs` and kept only the `track_frame` hook. `a_burst_to_a_dead_peer_holds_at_most_two_frames` then fails with "50 frames held for a dead peer", and the other 5 pass.
- **The hook is test-only.**
  - It is gated by `#[cfg(feature = "test-hooks")]` in `shared.rs`, `api.rs`, `send.rs` and `engine/mod.rs`. The only place that enables the feature is the self dev-dependency in `core/fsync-core/Cargo.toml:50`; no other crate does.
  - Production `clippy --lib` (no hooks) is clean.
  - How it counts: one `Weak` per frame built per publish. A frame is shared by all peers of that publish, so it counts clips still held, not per-peer copies. It reads 2 with one dead peer (running plus waiting) and 0 after close.
  - In test builds, `Weak<[u8]>` keeps each dead frame's allocation until the next `track_frame` prunes it. That is fine for tests.
- **Timing:**
  - The burst test is deterministic, apart from the existing assumption that a dial to a dead peer does not fail within `publish_wait` (200 ms). Busy publishes return at once.
  - `offline_peer_never_holds_up_a_publish` is exposed to M1, as described above.
  - 4 runs of `clip_offline_peer_publish` (6/6 each, about 8.1 s) all passed.

## FFI
- **Bindings:** I regenerated both from this branch: Swift with the repo's `uniffi-bindgen` on the debug `.so`, and C# with `uniffi-bindgen-cs 0.10.0+v0.29.4` using `--config core/fsync-ffi/uniffi.toml --no-format`. Both are byte-identical to the checked-in files. Against main, both differ only in the `contract_version` docstring and the checksum (18762 → 64396).
- **macOS check, read only:**
  - `nonisolated public static let` in an extension of a `@MainActor` class has precedent (`EngineBridge.swift:67`).
  - `.bootFailed(String, canReset:)` matches `AppPhase.swift:8`.
  - The guard runs before the engine is built, and only while `engine == nil`.
  - The wording matches Windows.
  - It should compile under Swift 6 tools. It is unreachable for checksum mismatches (M2).
- **Windows:** `ExpectedContract = 3`, and the test expects 3. For the catch filter gap, see M2.

## Docs
- protocol-spec step 3, system-architecture (contract v3, lane), codebase-summary, the changelog and the roadmap all match the code.
- ffi-contract also matches, except :10 and :229 (M2) and L5.

## File size
All changed files are under 500 lines. The largest is `system-architecture.md` (405, a doc); `send.rs` is 312.

## Commands run (in the scratch copy)
- `cargo fmt --all --check`: OK.
- `cargo clippy -p fsync-core -p fsync-ffi --all-targets -D warnings`: OK. With `--lib` (production features): OK.
- `cargo test -p fsync-core --lib`: 67 passed, 1 ignored.
- `clip_refusal_feedback`, `clip_protocol`, `service_facade`: all pass.
- `clip_offline_peer_publish`: 6/6, four runs.
- Mutations:
  - old queue: the burst test fails;
  - oldest-wins: slow-peer and unit tests fail;
  - pause between `finish_send` and `next`: `offline_peer_never_holds_up_a_publish` fails;
  - reorder fix: everything passes.
- I did not rerun the full 252-test workspace suite.

## Recommended actions
1. M1: reorder `lane.next()` before `finish_send` in `run_lane`. Optionally add a regression test with a pause hook.
2. M2: stabilise the `contract_version` doc comment (option a), widen the Windows filter (option b), and correct ffi-contract :10 and :229.
3. L3: assert `EngineBridge.expectedContract == contractVersion()` in a macOS test.
4. L5: move the `Published.pending` entry out of the "no version bump" section.

## Unresolved questions
- Is a crash at startup acceptable for a binding/library mismatch, given that they always ship together? If so, M2 drops to a docs fix.
- Windows: please confirm on the CI host or Windows host that a missing DLL surfaces as `TypeInitializationException`. That would make the existing filter ineffective, which is pre-existing.

**Status:** DONE_WITH_CONCERNS
**Summary:** The lane rewrite is sound, bounded and latest-wins-preserving, and the tests and mutation claims check out. One narrow ordering regression (M1, fix verified) and an unreachable contract check plus inaccurate docs (M2).
**Concerns/Blockers:** macOS and Windows CI are unverified here; Swift and C# are review-only.

---

# Re-review (fix round: edca5db, f2de096 on c10cd4a)

**Score: 9.6/10.** Critical 0, high 0, medium 0, low 1. **Approved** (bar 9.5 met), on the condition that the macOS and Windows CI jobs are green. Swift and C# were not compiled here.

I checked each claim myself, in a fresh `git archive` copy of the branch. After the mutations I diffed `send.rs` against the branch: it was back to the original.

## Previous findings
| ID | Verdict | Evidence |
|---|---|---|
| M1 | Fixed | `send.rs` `run_lane`: `job = lane.next()` now runs before `finish_send`. The pause hook `lane_report_pause` (cfg `test` or `test-hooks`, `config.rs`) sleeps after the report. With the old order restored and the pause in between, the new test `a_live_peer_is_never_busy_once_its_result_is_reported` FAILS at `tests/clip_offline_peer_publish.rs:257`, and the other 6 pass. The test is deterministic: a 1 s pause against back-to-back publishes. |
| M2 | Fixed | The `contract_version` doc comment is byte-identical to main's. The history moved to a `//` comment (`core/fsync-ffi/src/lib.rs:26-30`). I regenerated Swift (repo `uniffi-bindgen`, debug `.so`) and C# (`uniffi-bindgen-cs 0.10.0+v0.29.4`, `--no-format`, repo `uniffi.toml`); both are byte-identical to main and to the branch, with checksum 18762 again. `git diff main` on both binding files is empty. I also confirmed the premise: 02c31de (`Published.pending` appended) changed no checksum line. So a v2 binding loads against the v3 library and reaches the friendly check. |
| M2 (Windows) | Fixed | `App.xaml.cs:73-79`: the filter adds `TypeInitializationException` and unwraps it with `e is TypeInitializationException { InnerException: { } inner } ? inner.Message : e.Message`. That is valid C# 9; `inner` is definitely assigned. This also covers the earlier gap where a missing DLL arrived wrapped. |
| Docs (M2) | Fixed, one wording nit (R1) | `ffi-contract.md:10` now describes what really happens: a checksum mismatch is `fatalError` on macOS and a caught `TypeInitializationException` on Windows, and the doc comment stays stable. `:226-232`: the new "v3 (version bump)" section is accurate. The changelog is corrected. |
| L1 | Documented, accepted | The `LaneRunner` doc (`peer_send_lanes.rs:90-95`) states that the waiting clip is dropped without an unoffer, the publish sees "no answer", and the outbox expiry handles it. The reasoning is sound. |
| L2 | Fixed | The `Engine::clip_offered_to` hook (`api.rs`, `test-hooks` only) reads `offered_to` from a live outbox entry. The slow-peer test asserts that n+1's offer list is `Some([])` and n+2's is `[b]`. A mutation that only answers the superseded clip, with no unoffer, FAILS: "n+1 unoffered". The superseded unoffer runs synchronously inside the n+2 publish, so the assertion does not race. |
| L3 | Fixed (not run) | `EngineBridgeBootTests.testExpectedContractMatchesTheBundledEngine`. The file imports FsyncCoreKit, and both sides are synchronous and nonisolated inside a `@MainActor` class. It should compile; the macOS CI job runs it. |
| L4 | Fixed | `send.rs:66-67`: `(!opts.push).then(\|\| PublishInFlight::enter(self))`. A push is neither counted nor logged. |
| L5 | Fixed | The "v3 (version bump)" section holds `Published.pending`, and the section paragraph is rewrapped. Long lines elsewhere in that file already existed, so they are not a finding. |

## Remaining finding
- **R1 (low, docs):** the last sentence of `ffi-contract.md:10` is too general: "a binding one contract older or newer still loads and gets the friendly message".
  - It holds for v2↔v3, which I verified: no function signature changed, and no function was added or removed between main and this branch.
  - It does not hold for a future bump that changes or adds an exported function. A newer binding then checks a checksum or symbol the older library lacks (or has with a different value). macOS gets `fatalError`; Windows shows the inner exception (caught).
  - Suggested qualifier: "...still loads as long as the bump changes no exported function (true for v3)".

## Commands (scratch copy, `~/.cache/fsync-review02-*`)
- `cargo fmt --all --check` OK.
- `cargo clippy -p fsync-core -p fsync-ffi --all-targets -D warnings` OK. With `--lib` (no hooks): OK.
- `cargo test -p fsync-core --lib`: 67 passed, 1 ignored.
- `clip_offline_peer_publish`: 7/7 in 3 runs (about 8.1 s each).
- Mutations: old report order makes the M1 test fail (`:257`); no unoffer makes the slow-peer test fail ("n+1 unoffered").
- Bindings: regenerated Swift and C# are byte-identical to main.
- File sizes: every changed file is under 500 lines (largest code file: `send.rs` at 319).

**Status:** DONE
**Summary:** All findings are resolved and verified by mutation and binding regeneration. One low doc-wording nit is left (R1).
**Concerns/Blockers:** macOS and Windows CI results are still pending; Swift and C# were not compiled here.
