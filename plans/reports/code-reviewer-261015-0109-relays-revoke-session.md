# Review: core/relays-revoke-session (S3, S4) - score 8.6/10, 0 critical

Checks: cargo fmt OK, clippy -D warnings OK, cargo test -p fsync-ffi -p fsync-core all pass, swift test pass (xcframework current).

HIGH
- H1 apps/windows/src/Fsync.CoreBindings/Generated/fsync_ffi.cs (EngineConfig record, ~l.2837) + apps/windows/src/Fsync.App.Core/Bridge/EngineBridge.cs:57: relay_urls is inserted MID-record (core/fsync-ffi/src/engine.rs, after relay_url). uniffi records are serialized positionally: the checked-in C# bindings (not regenerated) paired with the new native lib would write a misaligned EngineConfig (silent corruption/crash; record layout is not covered by function checksums). On regeneration, the positional `new EngineConfig(coreDir, apiBase, relayUrl, deviceName, ...)` shifts and stops compiling (string passed as List<string>). Docs claim "existing hosts compile unchanged" - true for Swift labelled init only. Fix: regenerate C# bindings and update the call site in the same change/before any Windows lib build, or move relay_urls to the end of the struct (still needs regen for wire format). Correct the doc claim.

MEDIUM
- M1 PendingRevokeResume.swift:41-55 / EngineBridge+Revoke.swift:17-20: race between launch retry and sign-out. Retry reads pending A, revoke in flight (up to 10 s). If the user signs out meanwhile, logoutKeepingRevoke saves pending B (same Keychain account); the retry then calls clear() (deletes B) or savePendingRevoke(next) (overwrites B with A). B is lost/replaced (token stays valid up to 30 d). The current-session-revoked-after-relogin case you asked about cannot happen (retry only ever holds the token saved at an earlier logout), but B loss can. Fix: before clear/save, re-read and compare refresh token with the one retried; only touch if equal; or serialise via one actor/lock.
- M2 EngineBridge+Revoke.swift:12-16: when engine == nil, signOut runs a blocking revoke (<=10 s) and Keychain session is cleared afterwards; fine, but if `saved` is false (no session in Keychain, e.g. read error) nothing is revoked and the failure is only logged. Acceptable; note.
- M3 Network includes timeouts and DNS failure: 5 offline launches drop the pending revoke (token lives <=30 d). Matches spec; consider counting only launches where backend was reachable-but-failing, or a time-only limit.

LOW
- L1 Pending revoke reuses KeychainStore (AfterFirstUnlockThisDeviceOnly): good. Token never logged (description redacted, privacy .private errors). Core errors from revoke: no token in messages seen.
- L2 relays.rs: NodeBuilder::spawn parses with allow_http=true; scheme policy depends on Inner::open (inner.rs) running first. Fine for FFI/CLI; direct NodeBuilder users get no https policy (documented in comment).
- L3 CLI: --relay now value_delimiter ',' - URLs with commas unlikely; ok. Behaviour change for FSYNC_RELAY undocumented in CLI help beyond doc comment.
- L4 File sizes: no new source file >200 lines; existing tests >200 pre-exist.

OK
- revoke_session: validates before network, 401=success, rejects http unless allowed, refuses inside runtime, 10 s budget over whole call. Tests are real (local listener, real engine construction), no mocks except the documented `revoke` closure seam in Swift tests.
- Pending saved before logoutConfirmed; cleared only on true; once-per-launch retry off main; 5 failures / 30 d expiry.
- contract_version stays 2 (additive) reasonable for Swift.

Status: DONE_WITH_CONCERNS
Summary: Solid, tests/lint green; 0 critical. Below 9.5 due to C# wire/compile risk (H1) and pending-revoke overwrite race (M1).
Concerns/Blockers: Fix H1 and M1 to reach threshold. C# not buildable here, H1 assessed by inspection.
