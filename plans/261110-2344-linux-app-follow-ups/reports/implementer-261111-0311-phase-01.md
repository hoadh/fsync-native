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
