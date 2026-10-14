# Windows S5 (pending revoke) + S6 (relay list)

Branch `windows/s5-relays-revoke` pushed to origin (base ca466f7). Not merged.

## Commits
- 4486367 chore(windows): regenerate C# bindings for relay_urls, logout_confirmed and revoke_session
- ac17536 feat(windows): relay list in FSYNC_RELAY and a pending revoke kept at sign-out and retried at launch

## Changes (apps/windows only)
- Bindings regenerated (gen-bindings-cs.ps1 via schtasks; needed `cmd /c` in /TR for the `>` redirect). `EngineConfig.relayUrls` last, `IFsyncEngine.LogoutConfirmed`, `FsyncFfi.RevokeSession`.
- fsync_ffi.dll rebuilt (release) and copied to `Fsync.CoreBindings\runtimes\win-x64\native\`. It is gitignored (`.gitignore:14`), so not committed; any other machine must rebuild it with the same core.
- `AppSettings`: `SplitRelays`, `RelayUrls`, `IsLoopbackHttp`. `EngineBridge.InitializeAsync` passes `relayUrl: null, relayUrls: [...]`.
- `PendingRevoke.cs` (record, 5 failures / 30 days, ToString hides token), `SecretStore.PendingRevoke.cs` (save + token-matched settle under the existing store lock), `PendingRevokeResume.cs` (Outcome: NothingPending/Revoked/Kept/Dropped), `EngineBridge.Revoke.cs`.
- `LogoutAsync`: save pending (newest refresh token from store) -> `LogoutConfirmed()` -> settle only if true and token still the same -> clear session. No engine: revoke directly. FsyncException from logout is logged and swallowed (like macOS); session is still cleared locally and the pending entry stays for retry.
- Launch: `InitializeAsync` fires `ResumePendingRevoke` once per launch on the thread pool; never throws, never logs tokens.
- `EngineBridge` ctor got optional last param `revoke` (test seam). `FakeEngine`/`Harness` extended.

## Tests
- `dotnet test` whole solution: 140/140 (CoreBindings 9, Interop 47, App 84). Was 124; +16 new (`PendingRevokeTests.cs`: relay split/env, store token-matching, expiry, resume revoked/kept/5-failures/old/refusal, newer-token-kept on success and failure, sign-out ordering, unconfirmed/failed logout, newer entry not deleted, launch retry).
- `dotnet build` of the whole sln: 0 warnings, 0 errors.
- `cargo test -p fsync-ffi` on Windows: ok (2 + 2 passed), no Postgres needed.

## Notes
- Windows checkout had `windows/phase-14` checked out (clean); I created the new branch there from origin/main, so the checkout now sits on `windows/s5-relays-revoke`.
- Not tested: real backend revoke from the Windows app (no backend run).
- Files written with LF; git warns about CRLF normalization only.

## Unresolved
- None.

## Review fixes (report code-reviewer-261015-0240)
Commit: see `git log` on `windows/s5-relays-revoke` ("fix(windows): always finish sign-out ...").
- H1: `SavePendingRevoke` catches every Exception (type name logged), sign-out continues to `LogoutConfirmed`.
- H2: backend half catches every Exception (settle failure harmless: retry gets 401). `LogoutAsync` moved to `EngineBridge.Revoke.cs`, clears the session in `finally`; a failing `ClearSession` (IO/Unauthorized/Crypto) is logged, phase still goes SignedOut.
- M1: `SecretStore.SavePendingRevokeFromSession(apiBase, now)` reads refresh token and saves pending under one lock.
- M2 tests (new `PendingRevokeBridgeTests.cs`, bridge tests moved out of `PendingRevokeTests.cs`): relayUrls reaches EngineConfig with relayUrl null; engine-null direct revoke; failing save still logs out and signs out; non-Fsync exception in logout still clears session.
- Low: dead `catch (FsyncException)` removed from `SettingsViewModel.SignOutAsync` (LogoutAsync handles its own failures); README note that gitignored DLLs must be rebuilt with the bindings; `EngineBridge.cs` 189 lines, test files 144/115.
- Tests: full `dotnet test` 144/144 (CoreBindings 9, Interop 47, App 88). `dotnet build` sln 0 warnings/0 errors.
