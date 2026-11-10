# Phase 01: pairing "connection lost" flake

Status: DONE (local). The cross-OS X1+X2+X5 run is still open, for the user to run.
Branch `fix/pairing-flake`, not pushed.

## Root cause
The new device (the side that offers) emitted `PairingEvent::Completed` right after it wrote the last frame (`Done`), before the existing device had read it. `fsync-cli pair offer` exits on `Completed`. Its engine shutdown makes the iroh router cancel the pair/1 handler, and that drops the `Connection`: an implicit close with code 0 and no reason. QUIC throws away stream data that has not been sent yet. So the joiner, still waiting for `Done`, got noq `ReadError::ConnectionLost`, which displays as just "connection lost" (the source, "closed by peer: 0", was hidden). The list was already published and the new device had pinned it. Only the joiner reported a failure.

The flake has nothing to do with the relay, the tunnel or a "just-started endpoint". The ADR's working theory was wrong for this flake.

## Evidence
- Loop `apps/linux/tests/e2e/pair-flake-loop.sh` (fsync-cli <-> fsync-cli, fresh account per run). Baseline: **3 of 30 runs failed** (runs 16, 19, 23), plus 10 of 10 earlier passes, so 3 in 40 (the ADR says about 1 in 12). Logs are in `/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/baseline-b/` (and `baseline-30/` for the first 10 passes; its runs 11+ hit the backend OTP rate limit, so the loop now restarts the backend every 9 runs).
- The same signature in all 3 failures:
  - Joiner: `pairing failed: network: connection lost` after `list_advanced seq 1`, so the list was published.
  - Offerer: printed `paired`.
  - Joiner log: `conn::closed reason=ApplicationClosed { error_code: 0, reason: b"" }`.
  - Offerer log: `LocallyClosed` at the moment its router shutdown began. iroh logs "Shutting down remaining tasks" 0.6-0.7 s later, after `Endpoint::close`.
- Passing runs: the joiner closed first (`b"done"`).
- In-process (new test, new device closes its node on `Completed`). Before the fix, 7 of 40 rounds failed on the existing device with `network: connection lost: closed by peer: 0`, and 38 of 40 rounds lost the new device's own outcome (`shut down`, because the ticket was dropped).
- After the fix, every offerer log has `pair/1 responder drained closed=Some(ApplicationClosed(... reason: b"done"))`: 30 of 30 in the gate.
- Classification:
  - Which side closes: the new device (implicit drop).
  - At which step: after SignedList and Done, so after SPAKE and after the SAS.
  - Home relay: not involved.
  - Idle timeout / max_concurrent: not involved.

## Fix
- `core/fsync-core/src/pair/responder.rs`: `Completed` is now emitted in `run_responder` after the drain, which waits until the existing device closes the connection (bounded by 5 s, `DRAIN_TIMEOUT`). The existing device closes once it has read `Done`. Before the event, `ticket.finish` runs with no await in between.
- `error.rs`: new `CoreError::net_chain` keeps the source chain. Pair stream errors now read, for example, "connection lost: closed by peer: 0".
- `events.rs`: the doc of `Completed` states the new ordering.
- No retry was added.

## Windows "No addressing information available" (step 4)
This is a different cause and is not fixed. It is recorded separately in ADR 014, with these points:
- The offer waits at most 3 s for the home relay (`RELAY_WAIT`).
- iroh picks the home relay only when its first net report ends. When the QAD probes get no answer (the local test relay, and the TCP-only tunnel), that takes exactly `PROBES_TIMEOUT` = 3 s. Measured: bind to "home is now relay" was 3.002-3.003 s in every run.
- An offer that loses this race has no relay URL. A joiner without mDNS then gets iroh `NoAddress`, which is exactly that message.
- It was not reproduced locally, because the CLI starts its wait after `ensure_account` and wins.
- Added: a warning when an offer is published without a relay URL.
- The 120 s hang is not explained.

## Tests
- New: `core/fsync-core/tests/pairing_done_delivery.rs`, 12 rounds. The new device closes its node the moment it sees `Completed`. The test asserts that the existing device returns Ok, that the new device pinned the list, and that the window outcome equals the seq. Post-fix it also passed 60 of 60 rounds. No test hook was needed: the race shows up in-process often enough, and the pre-fix suite fails with near certainty through the outcome assert.
- `cargo fmt --all --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test -p fsync-core --no-fail-fast`: 241 passed, 0 failed, 3 ignored (47 binaries). One earlier full run failed `file_read_ahead::serial_small_reads_overlap_round_trips`, a CPU-timing test, while clippy was building in parallel. It passes alone and in the clean rerun, and it is unrelated.

## Gate
- After the fix, 30 of 30 consecutive clean runs. Logs: `/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/gate-30/` (binary with the fix, before the warn-log commit).
- Final tree (HEAD binary): **20 of 20** consecutive clean runs. Logs: `/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/gate-final-20/`.
- Not run: the app-based legs (`pair-with-cli.sh`, `clipboard-with-cli.sh`) and X1+X2+X5 on waso-vm (user).

## Commits
- `3bad29d` test(e2e): pairing flake loop
- `6184390` fix(core): new device reports pairing Completed only after Done is delivered (+ core test, `net_chain`)
- `1c9cb3b` fix(core): warn when a pairing offer has no relay URL
- `001640f` docs: ADR 014, changelog, roadmap
- this report and the phase todo: the next commit

## Docs
- ADR 014 "Known issues": the flake row is marked Fixed, with root cause and evidence. The Windows row stays open with its likely cause. "Open" was updated.
- `docs/project-changelog.md`: new 2026-11-11 entry.
- `docs/development-roadmap.md`: open items and the milestone row.

## Unresolved questions
1. Windows NoAddress: should `RELAY_WAIT` be raised above iroh's 5 s report timeout now, or only after X2 confirms it? (Check the new warning in the Windows CLI log, which needs `RUST_LOG`.)
2. `lib-app-instance.sh` `pair_apps` retries app <-> app pairing up to 5 times, and its comment blames "connection lost". It fails before the SAS, so it is not this bug. Should we investigate it separately, or try removing the retry once the user's runs are green?
3. Phase-03 note: an app in needs-pairing that joins shows "connection lost". Not reproduced here; the CLI refuses that join. Not looked into.

## Fix round (review code-reviewer-261111-0341, 7.5)

**H1: the test did not catch the bug.** Corrected.
- Where the round-1 numbers came from: a probe build of the round-1 test, run against the pre-fix responder (net_chain only). It reported 7 of 40 failures on the existing device and 38 of 40 lost window outcomes. The host was loaded at the time (loop and builds).
- Re-measured now in a scratch copy, with d4b226c `responder.rs` swapped in:
  - existing device: 0 of 40 failures, which matches the review's 0 of 192. That assert is timing-dependent and was not a reliable regression test.
  - window outcome (`shut down`): lost in 40 of 40 rounds. Through that assert the round-1 test did fail pre-fix, 3 of 3 runs here (`old_test.rs:71`).
- ADR 014 and the changelog are corrected.
- New deterministic test `new_device_reports_completed_only_after_the_existing_device_has_done`:
  - Test hook `InitiatorHooks.after_done`: the existing device waits 300 ms after reading `Done`, before it reports and closes.
  - Assert: the existing device's `Completed` comes before the new device's `Completed`. Then the new device closes its node, and the existing device must still succeed.
  - Pre-fix responder in the scratch copy (`/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/prefix-copy`, outside the repo): **failed 10 of 10 runs**. With the fix it passes.

**M1: the 5 s cap.**
- Docs reworded in `events.rs` and `responder.rs`: `Completed` comes after the joiner closed, or after at most 5 s.
- New test `completed_comes_after_the_drain_cap_when_the_existing_device_never_closes`: with `InitiatorHooks.keep_open`, the joiner reads `Done` and never closes. The test asserts `Completed` 4.5-6.5 s after the joiner finished. The pre-fix responder fails it (Completed comes at once).

**M3: shutdown during the drain.**
- A drop guard (`CompletedOnDrop`) emits `Completed` once the list is pinned, also when the handler future is dropped.
- `PairTicket::set_pinned` makes a dropped ticket report the pinned seq instead of `Closed`.
- I did not choose `ProtocolHandler::shutdown`: the guard covers every drop path.
- Test `shutdown_during_the_drain_still_reports_completed`: the joiner keeps the connection open, and the new node closes during the drain. Expected: `Completed`, then window outcome = seq.
- Without the guard (round-1 responder), the test failed 5 of 6 runs. It is not 6 of 6 because iroh's router sometimes lets the endpoint close end the drain before it cancels the handler (`run_until_cancelled` is biased to completion). With the guard it passes every time.

**M2: disagreement.** Documented in ADR 014 Known issues, with no behaviour change.
- A "re-check the backend list after a loss" mitigation cannot know whether the new device pinned. A false "paired" is worse than a false "failed".
- Pairing again is harmless: `listed_head` hands the head over without publishing.
- After the fix, this needs a real network failure.

**Low findings.**
- **L1:** `net_chain` cuts each source to 64 chars (`NET_SOURCE_MAX`), with a unit test.
- **L2:** the `Net` doc says its text is not stable. The changelog says so too.
- **L3:** only `FORCE_RELAY=1` forces relay.
- **L4:** the dead backend pid is removed from `PIDS` on restart.
- **L5:** `one_run` checks every step (mkdir, link, both logins plus the account state in their JSON, `JOIN_DELAY_S`).
- **L6:**
  - cleanup runs once (it resets the trap);
  - the log dir variable is now `PAIR_LOOP_OUT`;
  - logs are removed on an all-pass run unless `KEEP=1`;
  - the free_port race is documented, not changed.
  - shellcheck is not installed on this host, so it was not run. `bash -n` passes.
- **L7:** the changelog now says only a teardown-on-Completed offerer (the CLI) was affected.

**Checks (fix round).**
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- `cargo test -p fsync-core --no-fail-fast`: 242 passed, 2 failed, 3 ignored.
  - Both failures were `file_transfer` tests (200 MiB), with `Disk quota exceeded` on /tmp. The host /tmp quota is full, mostly from another job's 5.1 GB `review-target` in the shared scratch.
  - Rerun alone with `TMPDIR` on disk: 7 of 7 pass. Not related to this change.
- Loop gate (HEAD binary): **20 of 20** consecutive clean runs. Every offerer drain ended with the joiner's `b"done"` close (20 of 20).
  - Logs: `/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/gate-fixround-20/`
  - No containers or processes were left.
- The scratch copy with the pre-fix responder is at `/tmp/claude-1000/-home-alex-Work-fsync-native/ef54502c-e26e-4e9f-8211-6ea9740a777b/scratchpad/prefix-copy`, outside the repo. It contains debug prints.

**Fix-round commits.**
- `bb50a66` fix(core): Completed on shutdown during the drain; deterministic tests; net_chain cut; docs
- `908719e` test(e2e): loop script fixes
- docs (ADR 014, changelog) and this report: the next commit

**Unresolved (added).**
- The M3 test cannot fail 6 of 6 without the guard, because of iroh's cancel/close race. It does pass deterministically with the guard.
- shellcheck is not installed. Install it, or let CI lint the script?

## Fix round 2 (re-review code-reviewer-261111-0505, 9.0)

Build output and temp files for this round are on disk: `CARGO_TARGET_DIR=/home/alex/.cache/fsync-p01-target`, `TMPDIR=/home/alex/.cache/fsync-p01-tmp`, loop logs in `/home/alex/.cache/fsync-p01-logs/`.

**M-1: the loop no longer deletes a caller's log dir.**
- A caller-supplied `PAIR_LOOP_OUT` must be new or empty, and it is never removed. Checked by hand: a non-empty dir is refused with exit 2 and its contents are left as they were.
- Only the script's own `mktemp` dir is removed after an all-pass run, and only without `KEEP=1`.

**L-a: the pin now sets both flags at the same point.**
- `session` sets the guard's `pinned` and `ticket.set_pinned` together, right after the pin.
- Once pinned, the result is success everywhere (event, window, return value). That holds even if sending `Done` then fails, and no `Abort` is sent after the pin.
- No new test: forcing a `Done` write failure after the pin needs a fault hook inside the stream write, which is not cheap. The existing shutdown-during-drain test still covers the guard.

**L-b: the hook code is compiled only with `test-hooks`.**
- The `InitiatorHooks` branches (the `after_done` sleep and the `keep_open` 10 s spawn) are under `#[cfg(feature = "test-hooks")]`.
- The struct and field stay, so `run_initiator` keeps one signature. They are marked `#[doc(hidden)]` and are never read without the feature.

**L-c:** ADR 014 now calls the round-1 test timing-dependent and records the reviewer's 16 of 16 pre-fix passes.

**shellcheck:** run via `koalaman/shellcheck:stable -x`.
- Findings: SC1091 (sourced libs not followed) and SC2034 (`EXTRA`, which `cli()` in `cli-session.sh` reads).
- Fixed with `# shellcheck source=` and a scoped `disable` comment. Now clean (rc 0).

**Checks.**
- `cargo fmt --all` is clean.
- Production clippy (`-p fsync-core`, `-p fsync-cli -p fsync-ffi`, without test-hooks) with `-D warnings` is clean. `--workspace --all-targets -D warnings` is clean.
- Pairing tests pass: `pairing_done_delivery` 3/3, `pairing_hardening`, `pairing_iroh`, `e2e_pair_flow`, `pairing_repair_after_reset`, `pair_kdf`, and the lib `net_chain` test.
- The responder logic changed (pinned now means success), so the 20-run loop gate was rerun: **20 of 20** clean runs. Every drain ended with the joiner's `b"done"` close (20 of 20), and no containers or processes were left. Logs: `/home/alex/.cache/fsync-p01-logs/gate-round2-20/`.

**Commits (round 2).**
- `4238d72` fix(core): pin sets window and guard together; hooks behind `test-hooks`
- `10d37ed` test(e2e): loop never deletes a caller's log dir; shellcheck clean
- docs, the re-review report and this section: the next commit
