# Round 2: windows/s5-relays-revoke (27ed996 on ac17536)

Score: 9.5/10. 0 critical, 0 high, 0 medium, 3 low. Meets the threshold.
I did not run dotnet. Windows checkout is at 27ed996 and clean.

## Verification of round 1
- H1 fixed. `SavePendingRevoke` now has `catch (Exception)` (Revoke.cs, SavePendingRevoke). A failing save no longer aborts the logout. The new test blocks the tmp path with a directory, then asserts that `LogoutConfirmed` ran and the phase is SignedOut.
- H2 fixed. The whole body of `LogoutKeepingRevokeAsync`, including `engine is null`, `RunPendingRevoke`, `LogoutConfirmed` and `SettlePendingRevoke`, is inside `catch (Exception)` that logs only the type name. A test throws `ObjectDisposedException`.
- LogoutAsync (EngineBridge.Revoke.cs) is correct.
  - `try { await ... } finally { ClearSession; Phase = SignedOut; DevicesChanged }`.
  - ClearSession is wrapped in a catch for IO, UnauthorizedAccess and Cryptographic exceptions. `_data` is mutated before Save, so the in-memory session is cleared even if the disk write fails. The next launch restores the stale tokens, hits Auth, and signs out.
  - Phase is always SignedOut. The `LogoutKeepingRevokeAsync` body now catches everything, so nothing else can reach the finally and cancel it.
  - The finally does not swallow a later exception, because the inner try already catches the only call that can throw.
- M1 fixed. `SavePendingRevokeFromSession` reads Refresh and saves under one `_lock`, which closes the rotation window.
- M2 fixed. New tests cover relayUrls reaching the config with relayUrl null, the engine-null direct revoke, the failing save, and the non-Fsync exception.
- L1: the dead catch in SettingsViewModel is removed. The command now awaits `LogoutAsync` directly, which is safe because it does not throw.
- L2: file sizes are fine. EngineBridge.cs is 189 lines, Revoke.cs 85, PendingRevoke store 51. The tests were split into PendingRevokeBridgeTests.cs (115 lines), and PendingRevokeTests.cs shrank.
- L3: the README notes that the DLLs are gitignored and must be rebuilt when the bindings change.

## Remaining low items
1. `SavePendingRevokeFromSession` and `SavePendingRevoke` set `_data.PendingRevoke` before `Save()`. If Save throws, the in-memory entry survives and a later successful save (for example ClearSession) persists it.
   The worst case is one extra revoke of an already-revoked token, which returns 401 and is harmless. Assigning after a successful save would be tidier.
2. `WithoutAnEngineSignOutRevokesDirectly` (PendingRevokeBridgeTests.cs) can flake. The launch retry started by `InitializeAsync` runs on the thread pool. If it is scheduled after sign-out saves the pending entry, it revokes `ref-1` too.
   `revokes` then holds two entries, and the List is not thread-safe. The window is tiny. Fix by using `Assert.Contains` plus a lock, or by awaiting the resume.
3. Cosmetic: a trailing space in the Revoke.cs doc comment ("next launch. "), and now-unused `using Fsync.Core` / `Services` lines in SettingsViewModel.cs.

## Positives
All round 1 items were fixed with the correct semantics and a test for each. The sign-out can no longer leave a half-signed-out state.

**Status:** DONE
**Summary:** Round 1 findings are fixed correctly with no regressions. Score 9.5/10 with 0 critical, so it is OK to merge. The three low items are optional.
**Concerns/Blockers:** None blocking.
