# Round-2 review: relays + revoke session (45bc673, d806b3f, b70eec7)

Score: 9.5/10. Critical 0, High 0, Medium 0, Low 3. Meets the auto-approve threshold (>=9.5, 0 critical), but only just.

## Verification
- H1 fixed. relay_urls is last in core/fsync-ffi/src/engine.rs:34 with #[uniffi(default = [])]. Swift read/write converters, ==, hash and init in fsync_ffi.swift all use the same order, with relayUrls last and defaulted. Call sites (EngineBridge+Boot.swift:60 and the tests) use labels and compile unchanged. engine.rs:50 chain(relay_url, relay_urls) is unaffected. The repo has no UDL (proc-macro only), so there is no second order to keep in sync.
- Windows. The committed fsync_ffi.cs and apps/windows EngineBridge.cs:57 are untouched and still consistent with each other. Nothing breaks until the DLL and binding are regenerated together.
- M1 fixed. PendingRevoke.swift:66-70 re-reads the Keychain under entryLock and compares the refresh token before delete/replace. savePendingRevoke takes the same lock, so save and settle are atomic. The lock is never held across the network call. There is no re-entrancy: settle calls pendingRevoke(), which does not lock, and the private writePendingRevoke, which does not lock either. So no deadlock. The static NSLock is used only from detached or blocking contexts, with no await while it is held, so it is fine under Swift 6.
- Tests. swift test --filter PendingRevokeTests: 11 run, 0 failures. testNewerPendingSavedDuringRetryIsKept covers both success and Network failure.
- CLI help (core/fsync-cli/src/args.rs:25) now describes both the repeat and comma forms correctly.
- cargo clippy -p fsync-ffi -p fsync-cli --all-targets -- -D warnings: clean.

## Findings
Low
1. docs/ffi-contract.md:165-170 says what a C# host must do, but does not say that apps/windows is stale in this branch or name follow-up S5 (grep "S5" finds nothing in docs). Add one line: "apps/windows not updated in this branch (follow-up S5)".
2. EngineBridge+Revoke.swift:20 calls clearPendingRevoke() unlocked and unconditional, which is the same race class as M1. It is practically unreachable: it needs sign-in and a new sign-out while logoutConfirmed() is in flight, and the token it would remove was already revoked. For consistency use settlePendingRevoke(retried: saved, replacement: nil).
3. pendingRevoke() deletes unreadable JSON. It runs under the lock from settle and unlocked from run(). Harmless, since it only deletes garbage. A comment would be enough.

## Positive
Minimal lock scope. The field-order rationale is recorded in both the Rust doc comment and the contract. The regression test covers both outcomes.

## Unresolved questions
- Does S5 ship the new DLL and the regenerated C# binding together? An old binding against the new DLL fails at record decode.

**Status:** DONE_WITH_CONCERNS
**Summary:** H1, M1 and the CLI help fix are correct and complete, and I found no regressions. Clippy is clean and 11/11 PendingRevoke tests pass. Score 9.5, 0 critical.
**Concerns/Blockers:** Docs do not name S5 explicitly (Low 1). One unlocked clearPendingRevoke call remains (Low 2). Neither blocks approval.
