# M5 Trust-Reset Confirm API: Consent-Bound Recovery After Full Device Loss

**Date**: 2026-10-14 11:48
**Severity**: High
**Component**: fsync-core (trust, sync, service), fsync-ffi, fsync-cli, FsyncCoreKit, ADR 012
**Status**: Resolved for M5 scope on one Mac. Change is uncommitted. Open items below.

## What Happened

Built the recovery path for when every device is lost. A signed-in user calls `reset_account_trust` (step-up auth, new seq-0 genesis with only this device). Other devices see a different root, hold it as pending, and emit `AccountEvent::TrustReset`. The user answers with `confirm_trust_reset` or `decline_trust_reset`. Exposed via the service API, FFI v2 (`contract_version` 2) and CLI (`account reset-trust --yes`, `trust resets|confirm|decline`). Review went 7.5, 8.5, 9, then 9.5 (auto-approved).

## The Brutal Truth

The worst bug was silent. After a reset to a shorter chain, the incremental sync page came back empty and `sync_device_list` read that as "no change". Old devices never learned of the reset. No error, no log. Only a test that demanded the reset be visible caught it.

Round 1 was 7.5 with six Mediums, and several came from our own design. The first pending design let superseded roots accumulate, so a cap of 8 could evict a legitimate reset. The mDNS pairing tests also answered each other's codes when run in parallel (`BadMac`). We fixed that with a per-binary `lan_serial()` lock. That is not real isolation, and nextest could still collide.

## Technical Details

- Sync fix: `sync.rs` `sync_device_list`. An empty or non-connecting incremental page now falls back to a full chain fetch. The test failed without the fix.
- `reset_id` = hex hash of the offered *head*, not the root. If the chain grows, the event refires with a new id, and the old id returns `NotFound` for confirm and decline.
- Pending: one root. A different new root replaces it. The cap-of-8 design is gone.
- Declined: set of 32, only user answers add to it. Sync skips the full-chain fetch when the server head is a declined head.
- Own-reset guard: `begin_own_reset()` returns a `u32` counter, saturating decrement on Drop. It suppresses the `DeviceListChanged` push that our own PUT triggers.
- Retry: if the server list is already a seq-0 root listing only this device, pin it without a second PUT.
- Step-up: 401 `reauth_required` maps to `FsyncError::ReauthRequired`. Nothing changes locally. Backend wants a login under 5 minutes old. Budget is 3 resets per day; failed resets do not consume it.
- Partial failure message: `Internal("the server accepted the reset but this device could not pin it (...); call reset_account_trust again to finish it")`.
- Final gates: fmt and clippy `-D warnings` clean. `cargo test --workspace`: 253 passed, 0 failed, 1 ignored (1 GiB bench). xcframework built. Swift 8/8. e2e PASS: clip 0.03 s direct, 0.04 s relay; 256 MiB sha256-verified on both paths.

## What We Tried

- `reset_id` = root hash: rejected. The prompt could be stale, so confirm would pin a head the user never saw (review M2).
- Pending cap 8, declined cap 2: rejected. Junk roots evicted declines (forcing re-prompts) or legitimate pending entries (M3, N1).
- Boolean own-reset flag: concurrent resets cleared it early. Replaced with the counter (L2).
- Pin-failure test (T1c): skipped. It needs a read-only state dir or a fault-injecting store, which is a mock. The retry path is covered by the fresh-root shortcut test.

## Root Cause Analysis

Two causes. First, we modelled "which root is current" as a capped set, but an honest server serves exactly one chain. The caps invented new failure modes. Second, the sync layer treated an empty page as a valid answer without checking it against stored state. That is the same bug class as the P0 presence-Hello and history-pruning bugs.

## Lessons Learned

- A consent prompt must bind to the exact object shown. The prompt's identity must equal the identity of what gets confirmed.
- Empty does not mean unchanged. Any "no new data" branch has to verify against stored state.
- Do not add caps without a reason. Design for the protocol's real cardinality.
- Re-review every fix round. Each round introduced at least one new defect.

## Next Steps

- Persist pending and declined state across restarts. Owner: Dev A. Deferred by YAGNI. Today a restart re-prompts, and a CLI decline does not stick.
- Old devices learn of a reset only on their next `ensure_account`, because the backend closes their sockets. Pushing to them needs a backend decision. Owner: Dev A with backend. No date set.
- Pairing tests depend on mDNS on this host. Owner: Dev B. Needs a non-mDNS path or a CI check.
- Hostile server can alternate roots and force a re-prompt on every change. Unthrottled. Owner: Dev A. Decide before release.
- Tester flagged that the xcframework script regenerated tracked `fsync_ffi.swift` and nobody verified it is byte-identical. Owner: Dev A. Check before commit.
- Commit the M5 change once the regen check passes. No date set.

**Status:** DONE_WITH_CONCERNS
**Summary:** M5 trust-reset API is implemented across core, FFI v2 and CLI. The review reached 9.5 and every gate passes on one Mac. The silent sync bug is fixed with a regression test.
**Concerns:** Pending and declined state is in memory only. Old devices learn of resets late, not by push. Pairing tests need mDNS. The bindings regeneration is unverified. None of the open items has a date.
