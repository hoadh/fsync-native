# S3 + S4: multiple relays, revoke_session

Branch `core/relays-revoke-session` (from ca6d57d). Worktree `/Users/alex/Code/fsync-native/.claude/worktrees/agent-a7ed8cd0d93823a90`.
Commits: `45bc673` feat(core) relays + logout_confirmed + revoke_session; `d806b3f` feat(macos) pending revoke + regenerated bindings.

## S3 relays
- `core/fsync-core/src/net/relays.rs`: `parse_relays` (dedup, https only unless allow_http, rejects garbage) and `relay_map`. NodeBuilder builds `RelayMode::Custom(map)`; iroh picks the home relay by latency.
- `ServiceConfig.relay_url` replaced by `relay_urls: Vec<String>`; validated at `Inner::open` (http only with `allow_insecure_http`). force_relay unchanged.
- FFI additive: `EngineConfig.relay_urls: Vec<String>` (uniffi default `[]`); merged with `relay_url`, deduped. CLI `--relay` is repeatable / comma-separated.
- Tests: 5 unit tests (relays.rs); `core/fsync-core/tests/relay_failover.rs` with real in-process iroh-relay servers: one relay down but listed first, peers connect and echo over relay-only (PathState::Relay), plus both-up case. FFI test for validation.

## S4 revoke_session
- `service/revoke_session.rs` reuses `ApiClient::revoke` (401 = Ok), own runtime, 10 s budget. FFI free fn `revoke_session(api_url, refresh_token, allow_insecure_http)`. Errors: empty token or bad URL = Invalid, timeout or transient = Network.
- `logout_confirmed() -> bool` on the engine (true = revoked, 401 or final refusal; false = queued, or no session). `logout()` semantics unchanged.
- Tests (real backend plus TCP proxy): `tests/revoke_session.rs` (5), `logout_revocation.rs` asserts false when the backend is unreachable, FFI test.
- macOS (FsyncMacCore): `PendingRevoke` + `AppSecrets` Keychain account `pending_revoke`; `PendingRevokeResume.run` (revoked / kept(failures) / dropped; drop after 5 failures or 30 days; Invalid dropped); `EngineBridge.signOut` persists before `logoutConfirmed`, clears on true; boot retries once per launch, off the main thread.
- Swift tests: `PendingRevokeTests` (10, real Keychain and real FFI against a closed port). Integration `PendingRevokeIntegrationTests` (2: resume revokes the token, which then gets 401 on `/v1/auth/refresh`; signOut revokes and leaves no pending entry). Added to the second group in `run-fsync-mac-integration.sh`.
- docs/ffi-contract.md: additive sections plus the host pattern. ADR 014 not needed.

## Gates
- cargo fmt --check, clippy `-D warnings`: clean.
- `cargo test --workspace --no-fail-fast`: all pass (68 result lines ok). `file_read_ahead::serial_small_reads_overlap_round_trips` failed once in a fail-fast run under machine load (timing assertion) and passed on rerun and in the full run.
- xcframework rebuilt, bindings regenerated (`fsync_ffi.swift` committed). `swift test` FsyncMac: 206 tests, 0 failures (14 integration skipped without env). FsyncCoreKit package: builds, no XCTest cases to run.
- `run-fsync-mac-integration.sh`: PASS. `tests/e2e/p0-exit-demo.sh`: PASS.

## Deviations and concerns
- `revoke_session` has a third param `allow_insecure_http` (needed for the dev http backend).
- `contract_version()` not bumped (stays 2): changes are additive and the contract bumps only on breaking changes.
- `logout_confirmed()` chosen over an AccountEvent variant or changing logout's return type (both would break exhaustive switches or the signature).
- C# generation of the `#[uniffi(default = [])]` Vec field is unverified (no dotnet here); S5 should check it. apps/windows untouched.
- macOS `AppConfig` still has a single `relayUrl`; multi-relay host wiring is for S6.
- FFI test leaves some temp dirs in /tmp on success.
- This report is at the worktree path (plans/reports/ inside the worktree, uncommitted): writes to the main checkout path were refused by the worktree isolation guard. Copy it to /Users/alex/Code/fsync-native/plans/reports/ if needed.
