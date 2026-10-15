# Track A core round 2 - report

Branch core/round-2 (origin), based on main 2d87845. Commits:
- baa46dd feat(ffi): history_item_reps accessor and typed EngineAlreadyRunning error (A1+A3)
- c9de69c fix(core): fast redial after a peer crashes and returns on a new port (A2)
- 0e92f8c test(core): bound Postgres start, admin connect and service calls (A4)
- 55b898d test(backend): retry transient Mailpit HTTP resets (A4)
- a0149ef, c375e48, c79f3f5, 842a362: review fixes (dial recovery scoping, errors, naming)

## FFI (additive, contract_version stays 2)
- `FsyncEngine.history_item_reps(item_id: String) -> Result<Vec<Rep>, FsyncError>` (Swift `historyItemReps(itemId:) throws -> [Rep]`); Rep = {mime, data}.
- `FsyncError::EngineAlreadyRunning` unit variant, LAST in enum (uniffi index 14); text "invalid: another fsync engine is already running on this data directory". C# regen needed (new method + FsyncException.EngineAlreadyRunning).
- Outside ownership: one-line `ErrorText.swift` fix (exhaustive switch) and backend/tests/common/mailpit.rs retry.

## A2 root cause
After a crashed peer returns (same id, new port) iroh keeps a stale selected path while old connections are alive (held by Gate.live); new handshakes go to the dead path until idle timeout (15-30 s). Fix: first dial attempt 2 s; on stall drop older outbound same-ALPN Dialer connections (Gate::close_stale) and retry with current addresses (Discovery::addr_for). Measured: ~2 s. Tests: tests/dial_after_restart.rs (crash, graceful, ALPN scoping). Known gap: file/rep fetch paths still don't retry on a dead *cached* connection (clip send does).

## A4
- Port-publish flake already handled earlier (pg.rs). Added timeouts: container start 240 s, admin connect 5 s per try, service calls 90 s, status HTTP 10 s in logout_revocation. logout_revocation hang did not reproduce (8 isolated runs ok, 4 full-workspace runs ok); now fails loudly instead of hanging.
- Real flake found: backend smtp_mailpit `refuses_a_server_without_tls` failed 3 of ~8 runs (ConnectionReset on Mailpit HTTP just after start); fixed with retry in messages_to; 10+ later runs green.

## A5
Existing tests/relay_failover.rs covers "one of two relays unreachable" (passes). A mid-run relay death test was tried and removed: in-process relays lack QUIC address discovery so iroh never re-homes (45 s); not verifiable here. Relays do not mesh: peers whose only reachable relays differ cannot connect.

## Gates (final)
fmt ok; clippy -D warnings ok; cargo test --workspace --no-fail-fast 307 passed, 0 failed, 2 ignored; build-xcframework ok; swift test 208 tests, 14 skipped, 0 failures; mac integration PASS. Review 9.5/10, 0 critical (see code-reviewer-261015-core-round-2.md).

## Unresolved
- docs/ffi-contract.md + roadmap entries (lead).
- Does the app want dead cached conns handled in file/rep fetch (Track C/B or follow-up)?
