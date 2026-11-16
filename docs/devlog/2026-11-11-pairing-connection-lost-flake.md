# Pairing "connection lost" flake: root cause and fix

**Date**: 2026-11-11 06:25 (merge of PR #3, `381de6e`)
**Severity**: High
**Component**: `core/fsync-core` pairing (`pair/responder.rs`), `apps/linux/tests/e2e`
**Status**: Resolved (the Windows relay-race part is fixed but needs more runs to confirm)

## What Happened

Pairing failed about 1 time in 12 full runs with `Pairing failed: network: connection lost` on the joiner. The device list had already been published and pinned, so the account was consistent; only the joiner reported failure. Root cause: the offering device emitted `PairingEvent::Completed` right after it wrote the last frame (`Done`), before the joiner had read it. The CLI `pair offer` exits on `Completed`, its engine shutdown cancels the `pair/1` handler, and the connection closes with `ApplicationClosed { error_code: 0, reason: b"" }`. QUIC discards unsent stream data, so the joiner read "connection lost" instead of `Done`.

## Impact

A user who pairs a new device sees a failure after the account was already changed. Retrying is harmless, but it erodes trust in the flow. Affected all clients that pair through the shared core (CLI and apps), so the fix is in core, not the Linux app.

## Technical Details

- Reproduction: `apps/linux/tests/e2e/pair-flake-loop.sh` (fresh account per run). Before the fix: 3 of 30 runs failed. Each failure: joiner `pairing failed: network: connection lost` after `list_advanced seq 1`; offerer logged `paired`, then `LocallyClosed` when its router shutdown began.
- Fix in `pair/responder.rs`: `Completed` is emitted after a drain that ends when the joiner closes (it closes after reading `Done`), bounded by 5 s. A node shutdown during the drain still reports success because the pin is set first (`PairTicket::set_pinned`).
- Deterministic test: `core/fsync-core/tests/pairing_done_delivery.rs`, with the `InitiatorHooks` test hook (feature `test-hooks`). Pre-fix responder: fails 10 of 10 runs. Post-fix: passes.
- Verification: loop 30/30 and 20/20 in review rounds, 20/20 after fix round 2.
- Windows offers: a second race was found on waso-vm (4 of 8 offers failed with "No addressing information available"). The offer published before iroh's home relay was known, because `RELAY_WAIT` was 3 s and iroh's report timeout is 5 s. `RELAY_WAIT` is now 6 s (commit `aed5ec8`, a unit test pins it).

## What We Tried

- An in-process test first. It was timing-dependent: 7 of 40 failures pre-fix when the host was loaded, 0 of 40 after, and 3 of 3 failures in the scratch copy. It was replaced by the hook-driven test above.
- Adding a retry on the joiner. Rejected: it hides the ordering bug and would re-run a pairing that had already pinned.

## Root Cause Analysis

The completion signal was emitted on the wrong side of a network boundary. "Written" was treated as "delivered". The CLI made it worse: exiting on `Completed` tore the node down while the stream was still in flight. The bug only showed up as a rare flake because the teardown race is narrow, and the first test was timing-based, so it passed most of the time.

## Key Decisions

- Keep the 5 s drain cap rather than wait forever: a joiner that never closes gets `Completed` after 5 s. Trade-off: `Done` can still be lost if the joiner neither reads it nor closes within 5 s.
- Known and not changed: the two sides can disagree (the existing device reports `Failed` after publishing when `SignedList` or `Done` is lost). A false "paired" was judged worse than a false "failed".

## Lessons Learned

- Order-dependent failures need an explicit ordering hook in the test, not a sleep. Keep the deterministic test; the loop script is evidence, not a regression gate.
- Emit completion only after the peer has confirmed delivery, or document that it is not.
- A 1-in-12 flake needs about 40 runs before it can be called fixed. Count runs, not passes.

## Next Steps

- Confirm the Windows offer path on more runs (owner: Alex, next cross-OS X2 run). Pass criterion: no "No addressing information available" in 10 offers.
- Consider re-checking the backend list on a loss after `SignedList` before reporting `Failed` (no owner; needs a design decision).

## Unresolved Questions

- The 120 s hang seen in one Windows run (an orphan `pair offer` lingering up to 200 s) is not explained.
