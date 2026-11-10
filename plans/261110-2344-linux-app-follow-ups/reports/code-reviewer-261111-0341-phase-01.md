# Code review: phase 01 pairing flake (3bad29d..a30ddb6)

Score **7.5/10**. Critical 0, high 1, medium 3, low 8. NOT approved (bar 9.5). Persisted by the controller from the reviewer's hand-back. Builds ran on `git archive` copies; nothing was changed in the repo.

## Verdict
- **Root cause confirmed.**
  - `fsync-cli pair offer` returns on `Completed` (`cmd_pair.rs:30`).
  - Teardown then runs `Node::close` → `router.shutdown()`, which cancels the `accept` futures (iroh 1.3.0 `protocol.rs:608-617`), so the unsent `Done` is dropped.
  - Log evidence matches: the joiner sees `ApplicationClosed{0,""}`, passing runs see `b"done"`.
- **Fix is minimal and correct.** `responder.rs:60-83`.
- **Compatible with older joiners.** Every joiner has called `conn.close(0, b"done")` right after reading `Done` since 8420fde. Shipped apps therefore end the drain in about 1 RTT.
- **Checks:** clippy `-D warnings`, `fmt` and the `pair` tests are clean. Every file is under 500 lines.

## High
- **H1: `tests/pairing_done_delivery.rs` does not fail without the fix.**
  - Main's `responder.rs` swapped in: 192 rounds, 0 failures, both sequential and under CPU contention.
  - The "7/40, 38/40 pre-fix" figures in the report, ADR and changelog could not be reproduced.
  - Fix: make the ordering deterministic. For example, assert that the existing device has emitted `Completed` before the new device sees its own `Completed`, or add a test hook that delays the initiator's read of `Done`.
  - Correct or explain the numbers.

## Medium
- **M1: 5 s cap path untested, docs overstate it.**
  - On `Elapsed`, `Completed` is emitted anyway.
  - The docs at `events.rs:59-61` and `responder.rs:58-59,76` say "Done delivered", which is not guaranteed.
  - Fix: reword the docs, and add a test with a joiner that never closes (`Completed` arrives in about 6 s).
- **M2: outcome disagreement (pre-existing).**
  - Scenario: `Done` is lost, E times out after 15 s (`STEP_TIMEOUT`) and reports `Failed`. But E has already published the list and ingested trust, and N has pinned.
  - Fix: list it in ADR 014 Known issues. Optional mitigation: E re-checks the published list after a loss that comes after `SignedList`.
- **M3: shutdown during the drain loses `Completed` although N has pinned.** Fix: emit `Completed` from a drop guard once `r` is `Ok`, or implement `ProtocolHandler::shutdown` for `PairHandler`.

## Low
- **L1:** `net_chain` puts the peer-chosen close reason (up to about 1 KB) into `PairingEvent::Failed.reason` and the UI. Truncate it, or keep only the close code.
- **L2:** the `Net` Display text changed. No consumer matches the exact text. Document that it is not stable.
- **L3:** `pair-flake-loop.sh:75`: `FORCE_RELAY=0` turns forced relay on.
- **L4:** `pair-flake-loop.sh:59-61,31-32`: stale backend PIDs stay in `PIDS`, so cleanup can kill a recycled PID.
- **L5:** `pair-flake-loop.sh:101`: errexit is off inside `one_run` (called from `if`). Check return codes explicitly.
- **L6:** double cleanup on Ctrl-C; `free_port` TOCTOU; generic `OUT` variable name; logs are never removed.
- **L7:** the changelog overgeneralizes ("1 in 12 pairings"). Only teardown-on-`Completed` offerers (the CLI) were affected.
- **L8:** the no-relay-URL warning is fine.

## Unresolved questions
1. Where do the 7/40 and 38/40 figures come from?
2. M2: is "retry works" acceptable for this release?
3. M3: should `PairHandler` implement `shutdown`?
