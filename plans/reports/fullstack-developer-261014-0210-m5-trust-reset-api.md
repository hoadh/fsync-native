# M5 trust-reset API (ADR 012) - implementation report

## Files changed (uncommitted)
Core trust: core/fsync-core/src/trust/{manager.rs,reset.rs} (declined set, `decline_reset`, `pending_reset_infos`, `replace_with_genesis`; `pend_reset` skips declined roots).
Core service: service/{types.rs,types_history.rs (new, split to keep <200 lines),mod.rs,history_api.rs,error.rs,convert.rs,events.rs,trust_reset.rs (new)}.
Core sync fix: core/fsync-core/src/sync.rs `sync_device_list` (empty incremental page now triggers full-chain fetch).
FFI: core/fsync-ffi/src/{error.rs,types_account.rs,engine.rs}, tests/error_mapping_roundtrip.rs (ReauthRequired, VARIANT_COUNT 13).
CLI: core/fsync-cli/src/{args.rs,cmd_trust.rs (new),cmd_account.rs,main.rs,events.rs,ctx.rs}.
Tests: core/fsync-core/tests/{service_trust_reset.rs (new),trust_manager.rs}; apps/macos/Packages/FsyncCoreKit/Tests/FsyncCoreKitTests/FsyncCoreKitTests.swift.
Generated: FsyncFFI.xcframework, Sources/FsyncCoreKit/fsync_ffi.swift (rebuilt).
Docs: docs/{adr/012-account-recovery-trust-reset.md,ffi-contract.md,development-roadmap.md,system-architecture.md,project-changelog.md,api.md}.

## API
Service: `reset_account_trust() -> SvcResult<AccountState>`; `confirm_trust_reset(&str) -> SvcResult<AccountState>`; `decline_trust_reset(&str) -> SvcResult<()>`; `pending_trust_resets() -> Vec<TrustResetInfo>`.
Types: `AccountEvent::TrustReset { reset_id: String, devices: u32 }`; `TrustResetInfo { reset_id, devices }`; `FsyncError::ReauthRequired` (401 code `reauth_required`).
FFI Engine: same four methods. CLI: `account reset-trust --yes`, `trust resets [--json]`, `trust confirm <id>`, `trust decline <id>`; `clip watch` prints account events (structured JSON with --json).

## Results (final tree)
- cargo fmt --all --check: pass
- cargo clippy --workspace --all-targets -D warnings: pass
- cargo test --workspace --no-fail-fast: exit 0, 58 result lines ok, 0 failed
- build-xcframework.sh ok; swift build ok; swift test: 7 tests, 0 failures (incl. testTrustResetApiOnAFreshEngine)
- tests/e2e/p0-exit-demo.sh: PASS. No fsync processes left; no fsync/e2e containers left.
- No non-test source file >=200 lines.

## Deviations / decisions
- Bug fixed in sync_device_list: after reset to a shorter chain the incremental page was empty and read as "no change", so old devices never saw the reset. Test failed without fix.
- Added `declined` set (in-memory) so declined roots are not re-announced or re-listed.
- Confirm when no longer member: emits SelfRemoved, stops presence AND drops pin (`trust.clear()`), since pairing refuses over an existing pin. Returns NeedsPairing.
- Malformed reset ids return NotFound.
- Old devices learn of a reset only on next `ensure_account` (backend closes their sockets), not by push.
- contract_version left at 1.

## Unresolved questions
- Persist pending/declined state across restarts? Currently in-memory: restart re-prompts; CLI decline in a separate process does not persist.
- Bump FFI contract_version? Event field and error variant are source-breaking for Swift switches.
- macOS app UI handling of new event/error out of scope.
- If server accepts reset but local `replace_with_genesis` fails, retry spends reset budget (3/day).
- Pairing test uses mDNS on local host; may be environment-sensitive.
- Container `mystifying_euclid` (postgres:17, up 10h) predates this work; left untouched.

**Status:** DONE_WITH_CONCERNS
**Summary:** M5 trust-reset API implemented across core, FFI, CLI, docs with tests; all gates and e2e pass; nothing committed.
**Concerns:** In-memory pending/declined state, contract_version not bumped, sync_device_list behavior change (bug fix) touches shared sync code.
