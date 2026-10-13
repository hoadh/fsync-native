# P0 Rust Track: Core, FFI, CLI, Backend, and Swift Half of Phase 03

**Date**: 2026-10-14 02:00
**Severity**: High
**Component**: fsync-core, fsync-ffi, fsync-cli, backend, FsyncCoreKit
**Status**: Resolved for P0 scope on one Mac. Two-machine work, Windows/C#, and several phases remain open.

## What Happened

We built phases 01, 02, 06, 07, 08 and 09, plus the Swift half of 03, on one Mac. The final gate passed on a frozen tree: fmt, clippy `-D warnings`, 236 Rust tests passing with 0 failing and 1 ignored (a benchmark), xcframework build, Swift 6/6, and the e2e P0 demo. The core security review went from 6.5 to 9.6 over seven rounds and was auto-approved at 9.6 with 0 critical issues.

The review was not a clean run. Most of the rounds found real bugs in our own fix rounds, not just the original code.

## The Brutal Truth

We shipped a status report that claimed a 16-stream cap on the file server. The code had no such cap. The reviewer caught it by grepping for `Semaphore`. Trust the code, not the summary.

The `history.corrupt-*` pruning could delete the only copy of real history after two wrong-key startups. We wrote a test that locked the loss in. Round 4 caught the same class of bug again, in the pruning logic we had just "fixed".

The presence `Hello` was sent before the iroh relay URL existed. Relay-only clip delivery took about 20 s because nothing re-sent until the 20 s heartbeat. That should have been caught by a latency assertion from day one. It wasn't.

## Technical Details

- Relay clip latency: 20.4 s before, 0.28 s after `RELAY_READY_WAIT` (300 ms) plus address-change re-send.
- Round 3 introduced a 3 s pre-wait that regressed the direct round from 1.2 s to 4.05 s. Caught in round 2 review, fixed by shrinking the wait.
- `send_one` ack wait: a half-dead cached connection stalled `publish` for 5 s and was never forgotten. Fixed with a per-attempt deadline, `dialer.forget`, one retry, and no retry on peer refusal.
- Re-pair after history reset failed with `ProtoError::DuplicateDevice` in `DeviceList::next_add`. Fixed with a `listed_head` path that skips only the publish; SAS and MAC unchanged.
- Final e2e (one round, frozen tree): clip direct 0.04 s, relay 0.06 s; 256 MiB file 55.3 MiB/s send over LAN, 105.4 MiB/s via relay.
- Swift 10 MiB roundtrip: 37.8 ms in debug against a 20 ms target. Not re-measured in release.
- Flake: `devices_reset` hit testcontainers `PortNotExposed`. Patched with a 25x200 ms retry in `pg.rs`, which is not selective.

## What We Tried

- Pairing "already open" errors were swallowed inside a spawned task. Moved window open to the synchronous call path.
- Stale-connection stall: first fix used a timeout only; the reviewer showed the connection was never dropped. Needed forget plus close plus retry.
- Retention size-cap loop: a "break on zero freed" rule stopped too early. Replaced with an outbox-guard and own-blob selection.

## Root Cause Analysis

The core failure mode was fix rounds that were not re-reviewed or re-tested end to end. Each round fixed the previous findings and introduced one new defect. Agents reported "fixed" from reading diffs, not from running the code. Gate results were the coordinator's claims until the final tester run.

## Lessons Learned

- Verify agent status claims against the code. Grep for the thing claimed.
- Fix rounds add bugs. Re-review every round, and run the e2e after each one.
- Freeze the tree before the final gates, then run them once on that exact tree.
- Latency assertions belong in the e2e from the first commit, not after a 20 s surprise.

## Next Steps

- User: second Mac or PC for two-machine runs (phases 02, 06, 07, 08). Owner and date not set.
- Dev A: TrustReset confirm API (M5) and an FFI surface for history reset UI. Needed before the macOS app uses recovery.
- Dev A/B: chain checkpoint so fresh devices can sync past pruned history (backend keeps 100 rows per epoch).
- Dev B: selective `pg.rs` wait, `devices_reset` flake fix with 5 consecutive green runs; 10k WS load test; Postgres TLS.
- Dev C: release-build Swift roundtrip against the 20 ms gate.
- Tester: re-run `spikes/iroh-discovery` build on the final tree (not run after round 7).
- User: email provider (SES, Resend or Postmark) to replace the stub. Blocks real OTP.
- User: decide on git init. There is no `.git`, so CI and ADR merge status cannot be verified. Add `spikes/**/target` to `.gitignore`.
- Clean up six leftover `tmp.*` e2e directories under `$TMPDIR` after confirming they are not in use.

**Status:** DONE_WITH_CONCERNS
**Summary:** P0 Rust track is green on one Mac: 236 tests, Swift 6/6, and e2e passing on a frozen tree. Core review reached 9.6 with 0 critical. Phases 01, 02, 03, 06, 07, 08 and 09 are Partial, not Complete, pending two-machine evidence and the open items above.
