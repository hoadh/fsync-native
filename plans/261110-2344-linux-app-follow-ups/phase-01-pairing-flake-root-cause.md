# Phase 01 — Pairing "connection lost" flake: reproduce, root-cause, fix

## Context
- ADR 014 "Known issues" (`docs/adr/014-linux-desktop-app.md` around line 570):
  - L1 <-> devB pairing fails about 1 in 12 full runs with "Pairing failed: network: connection lost", right after the joiner signs in.
  - Windows offer / Linux join hung once and once failed with "No addressing information available".
- Rate seen 2026-11-10: 2 of 3 X2/X5 runs (`implementer-261109-2010-phase-06-x3-x4.md:98`).
- Code:
  - `core/fsync-core/src/pair/` (`initiator.rs`, `initiator_steps.rs`, `responder.rs`, `window.rs`)
  - `net/pair_handler.rs`, `net/dialer.rs`, `net/node_builder.rs`, `net/relays.rs`
- The text "connection lost" is not ours. It is most likely the quinn/iroh `ConnectionError::ConnectionLost` Display, mapped to `FsyncError::Net`.
- Harness: `apps/linux/tests/e2e/pair-with-cli.sh`, `cross-os-with-windows.sh`.

## Overview
- Priority P1 (user-visible on first device pairing). Estimate 3 d.
- Working theory (ADR): a just-started endpoint is not yet addressable via the relay over the TCP-only tunnel. The joiner dials before the relay has its home relay or address, and the connection is then closed.

## Steps
1. **Reproduce cheaply.** Write a loop script, `apps/linux/tests/e2e/pair-flake-loop.sh` (local only, two fsync-cli or app + cli, fresh data dirs, N runs). It writes per-run logs with `RUST_LOG=iroh=debug,fsync_core=debug`. Target a baseline rate over 30 runs. If the local rate is much below 1/12, add the tunnel conditions: relay-only (`--force-relay`) and added latency through a netns + `tc netem`, reusing the phase-15 netns helpers.
2. **Classify the failing runs** from the logs:
   - Which side closes, and with which close code or reason?
   - Was it during the SPAKE exchange or the window/ticket step?
   - Did the responder's endpoint have a home relay at dial time?
   - Is idle timeout or `max_concurrent` involved?
   Compare with passing runs.
3. **Fix at the cause.** Candidates, to pick from the evidence, not up front:
   - (a) the offer side waits for `home_relay` or "online" before it publishes the ticket;
   - (b) the joiner retries the dial or open-stream once on `ConnectionLost` before the first pairing byte (idempotent, because the SPAKE state is not yet consumed);
   - (c) keep-alive or idle-timeout settings on the pair connection.
   Rule: no blanket retry that would hide a real protocol error. A retry is allowed only before any SPAKE message is sent.
4. **Windows "No addressing information"** case: check whether the same cause (empty `NodeAddr` in the ticket) explains it. If so, fix (a) covers it. If not, record it separately in the ADR with logs.
5. **Tests:**
   - A core test with an injected delay before the relay connects (test-hooks), proving that pairing succeeds.
   - Plus the loop gate (plan.md Gates: 20 clean runs).
   - Plus one cross-OS X1+X2+X5 run on waso-vm.

## Todo
- [ ] Loop script + baseline rate (with logs)
- [ ] Root cause written into the ADR 014 "Known issues" row, with log evidence
- [ ] Fix + core test
- [ ] 20/20 loop, X1+X2+X5 PASS, results JSON in `docs/test-results/`
- [ ] Tester + review >= 9.5, PR, CI green

## Success
Root cause is named with evidence, the fix is in, and the gates pass. The ADR row moves from "Known issues" to a "Fixed" note.

## Risks
- Cannot reproduce locally: fall back to waso-vm X2 loops (slow, about 6 min per run). In that case, budget extra time.
- If the cause is in iroh: pin a fix or work around it in our dial code, and file it upstream. No fork.

## Rollback
Revert the PR. Pairing behaves as before (flaky, but working on retry by the user).
