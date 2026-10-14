# Review: windows/s5-relays-revoke (4486367, ac17536 on ca466f7)

Score: 8.8/10. 0 critical, 2 high, 3 medium, 3 low. Below the 9.5 auto-approve bar.
Scope: apps/windows excluding Generated. Bindings skimmed: relayUrls is last with a null default, LogoutConfirmed and RevokeSession are present. Positional call is correct. Windows checkout confirmed at ac17536. I did not run dotnet.

## High
1. EngineBridge.Revoke.cs:64 - SavePendingRevoke catches only IOException and CryptographicException.
   `File.Move(..., overwrite: true)` and `WriteAllBytes` on Windows commonly throw UnauthorizedAccessException when AV or an indexer holds the file. That is not an IOException.
   It then escapes before `LogoutConfirmed` runs, so the sign-out aborts: no engine logout and no ClearSession.
   SettingsViewModel.cs:57 catches only FsyncException, so the exception reaches the AsyncRelayCommand and probably crashes the app.
   macOS catches everything here. Fix: `catch (Exception e)` and log the type name, which is also what RunPendingRevoke does.
2. EngineBridge.Revoke.cs:28,30 - `SettlePendingRevoke` (Save, so IO/DPAPI/UnauthorizedAccess) and `LogoutConfirmed` (ObjectDisposed/InvalidOperation if the engine is disposed during a reset) are not covered by `catch (FsyncException)`.
   Any other exception skips `_secrets.ClearSession()` in LogoutAsync. The engine has already logged out, but the UI stays signed in with dead tokens.
   Fix: widen the catch to Exception, as macOS does (`try?` plus a catch-all), so ClearSession always runs. Failing the settle is harmless because the retry returns 401, which counts as revoked.

## Medium
3. EngineBridge.Revoke.cs:59-61 - Reading `Session` and saving the pending entry are two separate lock acquisitions.
   A token rotation arriving on the callback thread between them leaves the pending entry holding the pre-rotation token.
   If the logout is then unconfirmed, the retry revokes the old token (401, "revoked") and deletes the entry, while the newest token may still be valid.
   macOS has the same window. A `SecretStore.SavePendingFromSession(apiBase, now)` that reads and writes under one lock closes it cheaply.
4. Tests miss several behaviours.
   - Nothing asserts that the EngineConfig reaching the factory has relayUrls set and relayUrl null (the harness factory ignores cfg).
   - The engine == null direct-revoke path is untested.
   - There is no test for a failing Save or for a non-Fsync exception during logout, which are findings 1 and 2.
   - A same-token retry versus new sign-out race is untested. Only the different-token case is covered.
5. PendingRevokeResume.cs:48 - A catch-all `FsyncException` drops the token, which matches macOS and the contract (Invalid is final).
   It also drops on Internal and other unexpected cases. Acceptable. Consider logging at Warn with the type name only, which is already done.

## Low
6. SettingsViewModel.cs:57 - The `catch (FsyncException)` is now dead because LogoutAsync no longer throws it. Update it to surface unexpected errors or remove it.
7. File sizes are OK but tight: EngineBridge.cs is 197 lines and PendingRevokeTests.cs is 198. Any follow-up needs a split.
8. fsync_ffi.dll is gitignored and was copied by hand. Document, or make the build fail loudly, when the binding and DLL are out of sync. The contract says an old DLL fails at record decode.

## Answers to the specific questions
- Swallowing FsyncException in LogoutAsync is acceptable and matches macOS. The entry stays for retry and the local session is cleared.
  It needs finding 2 so that other exceptions do not leave a half-signed-out state.
  If the save failed (saved == null) and logout failed, the token is lost with only a log line. Same as macOS.
- DPAPI writes: Save is under `_lock`, writes a tmp file, then does an atomic `File.Move` overwrite. This is fine, though not fsynced. The failure mode is finding 1.
- Concurrent sign-out and launch retry: SettlePendingRevoke compares Refresh under the lock, and the lock is never held across the network call. This is correct.
  With the same token, a late Kept settle overwrites CreatedAt and Failures with the older values. Harmless.
- UI threads and deadlocks: none found. All blocking work is in Task.Run, there is no .Result or .Wait, and `_revokeResumed` is only touched from InitializeAsync.
- Secrets: ToString is overridden, logs carry exception type names only, and JSON round-trips via the positional record (covered by the reopen test). No leaks found.
- Relay split: trimmed, empties dropped, and an empty list when unset. The core merges and dedupes. Correct.

## Positives
Faithful port of the macOS design. The ordering (save, then logout, then settle) is correct. The token-matched settle has tests. The never-throw wrapper at launch is good. Relay handling is minimal and clean.

## Fix list to reach 9.5
Findings 1 and 2 (catch-all, about 3 lines), plus a test for each. Optionally finding 3 and the relayUrls assertion in finding 4.

## Unresolved questions
- Is AsyncRelayCommand in this app configured to route unhandled exceptions somewhere safe? If so, finding 1 is less severe.

**Status:** DONE_WITH_CONCERNS
**Summary:** The design matches macOS and the races are handled correctly. Score 8.8/10, 0 critical.
**Concerns/Blockers:** Two high findings. A non-IOException from the secret store (UnauthorizedAccessException) or a non-Fsync exception during logout aborts sign-out and skips ClearSession. Widen the catches, add tests, then re-review.
