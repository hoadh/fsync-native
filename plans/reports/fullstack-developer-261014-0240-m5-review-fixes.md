# M5 trust-reset review fixes (round 2)

## Design choices
- **reset_id = hash of the offered head** (was: root). Consent is bound to what was shown (M2): if the offered chain grows, the pending entry takes the new head, the event fires again (new id, new names) and the old id gives `NotFound` for confirm and decline. Declines stay keyed by root (a declined root stays declined when it grows). Documented in ADR 012.
- `prompted` set removed: the pending entry records what was announced (less state).
- `TrustManager::confirm_reset(head) -> Result<Option<u64>>`: `None` = not pending, so a confirm/decline race is `NotFound` (M6). Pending is only cleared after a successful pin.
- Pending cap 8, new roots dropped when full (announced ones kept, warn log). Declined: own cap 32, only user answers add (M3).
- Own-reset guard `TrustManager::begin_own_reset()` (in-flight flag in `Inner`, checked in `pend_reset` under the inner lock, cleared on Drop) held across get-list, PUT and replace (M5).
- `reset_account_trust`: WrongAccount pre-check before any server call; if the server's list is already a seq-0 root of only this device, pin it without a PUT (retry path); local pin failure after the server accepted returns `Internal("the server accepted the reset but this device could not pin it (...); call reset_account_trust again to finish it")` (M4).
- L1: when the incremental page is empty/does not connect, `sync_device_list` does `GET /v1/device-list` and skips the full chain if its head is a remembered declined head (declined entries store the last head seen).

## Items
M1 `device_names` in `TrustResetInfo` + `AccountEvent::TrustReset` (core, FFI, CLI text/JSON, Swift test). M2-M5 above. M6 tests below. L3 `contract_version()` = 2 (ffi lib, ffi-contract.md, Swift test). L4 strict 64-hex `parse_reset_id` (no trim). L5 `is_self_removed` -> false on poison. L6 `tracing::warn` on undecodable pending entries. L7 `mdns: false` where no pairing happens (pairing needs mDNS on this host: "No addressing information available" without it, so pairing tests keep it).

## Files
Src: core/fsync-core/src/{trust/{manager,reset,mod}.rs, events.rs, sync.rs, service/{trust_reset,convert,events,types}.rs}; core/fsync-ffi/src/{lib,types_account}.rs; core/fsync-cli/src/{events,cmd_trust}.rs. All < 200 lines (types.rs 194).
Tests: core/fsync-core/tests/{trust_manager.rs (updated), trust_reset_manager.rs, sync_device_list_reset.rs, service_trust_reset.rs (updated), service_trust_reset_edges.rs, service_harness/mod.rs (shared helpers, `lan_serial()` lock)}; Swift FsyncCoreKitTests.swift.
Docs: docs/{adr/012-account-recovery-trust-reset.md, ffi-contract.md, api.md, system-architecture.md, project-changelog.md}.

## New tests (all real backend / real engines, no mocks)
- Ready branch: same identity re-installed resets a 2-device list; old device confirms -> `Ready`, no SelfRemoved.
- Grown offer re-announced with new names; shown id -> NotFound (confirm and decline); new id confirms.
- Pinned initiator (A+B paired, A resets): A sees no TrustReset, B does. Manager-level: guard suppresses, foreign root after guard is announced.
- WrongAccount: device pinned to account 1 signed in to account 2: `Invalid`, pin untouched, account 2's list not replaced. Manager-level WrongAccount (ingest/replace) already covered in trust_manager.rs.
- Retry/budget: 5 consecutive resets all `Created` (limit is 3/day).
- Declined-set eviction resistance: 100 junk roots after a decline, declined root stays declined when it grows; pending cap keeps oldest 8.
- sync.rs: empty incremental page after reset to a shorter chain -> `ResetPending`; decline sticks incl. after growth (`is_declined_head`).
- confirm/decline thread race: exactly one wins, other `NotFound`.
- Flake found and fixed in the harness: concurrent mDNS pairing tests answered each other's codes (`BadMac`); mdns tests now hold `lan_serial()`. Edge file passed 4/4 reruns.

## Acceptance (actual output)
1. `cargo fmt --all --check`: clean (FMT_OK). `cargo clippy --workspace --all-targets -- -D warnings`: Finished, no warnings.
2. `cargo test --workspace --no-fail-fast`: exit 0, 61 `test result: ok` lines, 0 failed.
3. `scripts/build-xcframework.sh`: ok. `swift test`: "Executed 8 tests, with 0 failures".
4. `tests/e2e/p0-exit-demo.sh`: `[e2e] PASS` (first attempt failed only because `cargo` was not on PATH in that shell; rerun with `source ~/.cargo/env`).
5. No fsync/backend/relay processes left; only `mystifying_euclid` (plus unrelated pre-existing containers) running; no fsync temp dirs. The failed first e2e attempt left `tmp.nRJN1rETfn`, removed. Remaining `tmp.*` dirs in $TMPDIR predate this work (Sep 26 and 11:07 with an old results.txt), left alone.

## Notes / unresolved
- Backend source says reset success pushes `DeviceListChanged` to the account's online sockets (api.md), so the M5 race is real; guard covers it. Failed resets do not consume the 3/day budget (backend test), so the retry shortcut mostly saves a round trip and covers the identical-genesis case.
- A device that only ever gets the reset by `ensure_account` still learns late (unchanged, ADR 012).
- Behaviour change noted in changelog: reset-to-shorter-chain now surfaces as a pending reset rather than silent "no change".
- Event `device_names` are signer-chosen text; ADR/ffi-contract say to show as untrusted.
- Not done on purpose: persisting declines (YAGNI).

**Status:** DONE
**Summary:** All review findings (M1-M6, L1, L3-L7) fixed with real-backend tests and docs; fmt, clippy, workspace tests, Swift tests and e2e all pass.
**Concerns:** reset_id semantics changed (head hash, not root hash); FFI contract bumped to 2. Pairing tests require mDNS and now serialize per test binary.

## Round 2 fixes

Changes
- N1: `pend_reset` keeps one pending root; a different root clears the old entry and replaces it (never dropped). `MAX_PENDING` removed. Test renamed `a_new_root_replaces_the_stale_pending_ones` (latest pending, stale ids -> confirm `None`/decline `false`, 10 roots = 10 prompts). `trust_reset_manager` junk test now expects 101 prompts. ADR 012 + changelog updated.
- N3: `is_own_fresh_root` is now a free fn in `service/trust_reset.rs` taking account + me; checks `account_id`.
- L1: verified, no change: same head returns before emit, so no duplicate emit.
- L2: `resetting` is a `u32` counter (inc in `begin_own_reset`, saturating dec on drop); simpler than a lock.
- L4: doc on `confirm_reset` says all pending entries are cleared.
- L5: `confirm_trust_reset` logs `restart_presence` failure with `tracing::warn!`, returns the status.
- T1a: unit test with real signed lists: wrong account, other device's root, multi-device list all rejected.
- T1b: `alternating_resets_each_do_a_real_put_until_the_daily_budget_is_spent`: two logged-in devices alternate 3 resets (3 real PUTs); the 4th gives `Network("... 429 rate_limited")`.
- T1c: skipped. A pin failure needs a failing SQLite trust store; a read-only state dir is OS/root dependent and fragile, and it would need a fault-injecting store (a mock). The retry path is covered by the existing fresh-root shortcut test.
- mDNS lock: comment says it is per test binary; nextest/parallel runners can collide.

Results
1. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean.
2. `cargo test --workspace`: all pass, 0 failed.
3. `scripts/build-xcframework.sh` OK; `swift test`: 8 tests, 0 failures.
4. `tests/e2e/p0-exit-demo.sh`: PASS.
5. No fsync processes or temp dirs left; `mystifying_euclid` untouched.

Docs impact: minor (ADR 012, changelog).

**Status:** DONE_WITH_CONCERNS
**Summary:** N1, N3, L1, L2, L4, L5, T1a/b and the mDNS comment are done; all acceptance checks pass.
**Concerns:** T1c skipped (needs fault injection). `tests/trust_manager.rs` is over 200 lines (pre-existing, now 304). Pending state holds one root, so a hostile server that alternates roots can keep replacing the prompt (each replacement re-announces); not throttled per YAGNI. An unrelated zsh process from another session is running; not mine.
