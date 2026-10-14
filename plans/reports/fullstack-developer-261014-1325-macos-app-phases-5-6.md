# Report: macOS app phases 5-6 (settings, bundle, integration, docs)

## Result
- `apps/macos/FsyncMac`: `swift build` has 0 warnings, including a forced full rebuild with tests. `swift test`: 68 tests, 0 failures, 3 skipped (the integration tests, as `FSYNC_IT_API` is unset). Before this work: 59.
- FsyncCoreKit `swift test`: 8/8.
- `run-fsync-mac-integration.sh` PASS: 3 integration tests, 0 failures, ~15 s of test time.

## Files
All new files are under apps/macos unless noted.
- Core:
  - `Settings/LaunchAtLogin.swift`
  - `Settings/PairingPasteboard.swift`
  - `Bridge/EngineBridge+Settings.swift`
  - `Bridge/KeychainStore.swift` (modified)
  - `Features/Devices/StatusSummary.swift` (rewritten)
- UI:
  - `Settings/SettingsView.swift`
  - `MenuPopover.swift`, `Menu/MenuBarContentView.swift`, `Pairing/PairOfferView.swift` and `Devices/DeviceRowView.swift` (modified)
- App: `FsyncMacApp.swift` adds a `Settings` scene and `showSettings`.
- Tests:
  - `SettingsTests`
  - `Integration/DevBackendHarness.swift` (172 lines)
  - `Integration/FsyncMacIntegrationTests.swift` (163 lines)
  - Also modified: `KeychainAndSecretsTests`, `DevicesModelTests` and `OnboardingRenderTests`.
- Scripts and ignore file:
  - `scripts/build-fsync-mac-app.sh` (67 lines)
  - `scripts/run-fsync-mac-integration.sh` (120 lines)
  - `.gitignore` (`build/`)
- Docs: codebase-summary (repo map, tests, new "Running the macOS app" section), system-architecture (macOS app shell section), project-changelog, development-roadmap.
- Plan: phase 5 and 6 todos and statuses, and plan.md. plan.md status is `in-progress`.

## Extra items
1. **Pasteboard markers.** `PairingPasteboard.copy` writes the string plus empty `org.nspasteboard.ConcealedType` and `TransientType` data. The tests use a private named `NSPasteboard`: both markers are present, and the previous contents are replaced.
2. **"Settings…"** is in the popover (ready and non-ready layouts), next to Quit. It uses a `Settings` scene.
   - The app opens it with `NSApp.sendAction(showSettingsWindow:)` on macOS 14+ and `showPreferencesWindow:` on 13.
   - I did not check the window visually.
3. **Integration scenarios.** The three tests are:
   - `testFirstDeviceJoinStatusAndRemove`:
     - Mac OTP login: the OTP is read from the backend log, and the Mac becomes the first device (`isFirstDevice`).
     - CLI login gives `needs_pairing`.
     - CLI `pair offer` / Mac `join`: SAS words equal and the node id matches. The Mac confirms, the CLI exits 0 with `paired`.
     - CLI `clip watch` brings the device online in 0.3 s (limit 5 s).
     - Remove: the CLI is gone from the list and the Mac stays.
   - `testMacOffersAndCliJoins`: the CLI is created first and the Mac gets `needsPairing`. The Mac offers a code, the CLI joins, SAS words are equal, then the Mac is `.ready` with 2 devices.
   - `testRecoveryResetsTrust`: the Mac is `needsPairing`, then explain, final confirm and `resetAccountTrust`. Result: `.ready`, `isFirstDevice`, no other devices listed.
4. **Cleanup.**
   - The trap removes processes, `docker rm -f -v` of its own `fsync-mac-it-pg-$$`, and leftover Keychain services `app.fsync.mac.tests.*`.
   - It also runs `pkill -f $WORK` for CLI peers left behind.
   - The swift test run is backgrounded with `wait`, so SIGTERM triggers cleanup at once.
   - I checked it after a PASS and after a SIGTERM mid-test: no `fsync-mac-it` container, no backend, relay or CLI processes, 0 test Keychain items. Logs stay in the work dir on failure.
   - `mystifying_euclid` was never touched and is still present.
5. **Bundle.**
   - Built with `--out` in the temp dir.
   - `codesign -dv`: `Identifier=app.fsync.mac`, `flags=0x10002(adhoc,runtime)`, `Signature=adhoc`. `codesign --verify --strict` passes.
   - `plutil -lint` OK. `LSUIElement` is true, and `FsyncApiBase` is set.
   - `open` ran it as `ApplicationType=UIElement` (no Dock icon). `osascript quit` exited it cleanly through `terminateLater`.
   - Cleanup: the bundle, the 3 Keychain items under `app.fsync.mac` and the data dir are deleted. I checked that none remain.

## Findings and fixes
- **Real bug, fixed.** `KeychainStore.set` raced.
  - The login model and the engine's `onSessionChanged` both saved the session. Both saw "not found" and both added, so one got `errSecDuplicateItem`.
  - The user would see "Couldn't store your sign-in securely" and end up signed out.
  - The fix retries as an update on a duplicate.
  - A new concurrency test fails without the fix (3 of 4 writers failed) and passes with it.
- **Status semantics (deviation).**
  - The core's `path_state` reports Lan/Relay only for a live QUIC connection, and nothing in the shell connects (sync is phase 12). A paired, online CLI therefore stays `status == .offline` while `online == true`.
  - I changed `StatusSummary` and `DeviceRowView`. A device with `online && status == .offline` now counts as `present` and shows "Online" in blue, not "Offline".
  - `anyPeerOnline` (the icon) now includes presence.
  - The integration test asserts `Device.online` within 5 s (measured 0.3 s), not `.lan`. This differs from the plan text, which expected `.lan`.

## Deviations
- The three tests merge plan scenarios 1-3 into one flow, because status and remove need the pairing state.
- Render tests: the `ViewRenderTests.swift` file was not created. The views are rendered in the existing `OnboardingRenderTests` (Settings added). Login/Otp/PairJoin stay unrendered (`@FocusState`).
- README is outside the allowed paths, so the "build and run" mention is in codebase-summary only. `docs/ffi-contract.md` was not edited.
- The `Settings` scene uses `NSApp.sendAction` rather than `SettingsLink`, because there is no view hierarchy in the menu and the app must support macOS 13.

## Not done or not verified
- The manual UI checklist: MenuBarExtra focus, the remove and trust-reset dialogs, the Settings window, the launch-at-login toggle, and sign-in from the bundle. These todos stay unticked.
- The code-reviewer pass (≥9) was not run.
- The Lan or Relay status flip is not tested.
- The offline flip is not tested (as the plan notes).

## Unresolved questions
1. Should the app dial peers by itself (for example a keepalive connection) so Lan/Relay shows without clipboard traffic? Today that is only visible once phase 12 sync connects.
2. The first-launch Settings window and `showSettingsWindow:` behaviour on macOS 14+ for an accessory app need a visual check by someone with a screen.
3. Should the README get the build and integration script lines? It is outside this task's path limit.
4. Earlier open question, still open: signing for the data-protection Keychain.

**Status:** DONE_WITH_CONCERNS
**Summary:** Phases 5-6 are implemented. The integration script PASSes with the CLI as peer, both SAS directions, online status in 0.3 s, remove, and recovery. `swift test` gives 68 tests (3 skipped) in FsyncMac and 8 in FsyncCoreKit, with 0 warnings. The bundle script output was checked and launched. The integration run found and fixed a real Keychain write race.
**Concerns/Blockers:** The status assertion is `online`, not `.lan`, because the core only reports Lan/Relay for live connections. Manual UI checks and the code review are not done. The Settings window and bundle sign-in were not verified visually.

## Round 1 fixes

Status: DONE_WITH_CONCERNS (Medium 5 not verifiable here). All paths under `apps/macos/FsyncMac/Sources/...` unless noted.

### Per finding
- **High 1** `FsyncMacCore/Bridge/EngineBridge+Boot.swift`: `guard !Task.isCancelled` after every `EngineCall.run` (setSession/start, ensureAccount, makeEngine failure) before any `fail`/`banner`/`phase` write. `started` is still recorded. Session read in `runBoot` moved off the main actor (found while testing the bundle: a blocked Keychain read froze the main thread). Test `testSignOutDuringAccountCheckLeavesSignedOutAndNoBanner`.
- **Medium 1** `KeychainStore.swift`: probe treats `errSecDuplicateItem` as supported (`isSupported(addStatus:)`); DP-mode `get` miss reads file keychain and moves the item (`readMigrating`, copy failure keeps legacy + returns data); `delete` removes both copies; new `add` (false on duplicate). Tests: probe statuses, 4 migration cases + read error propagation, `add`. Documented in `docs/system-architecture.md`. The real DP path cannot run unsigned (-34018), so migration is tested through the injectable closures only.
- **Extra finding (Medium-level)**: concurrent first `SecItemAdd` of the same item deadlocked the file keychain (seen via `sample` in the new concurrent-writer test, 16 min hang). All `KeychainStore` calls now take one process-wide `NSRecursiveLock`. The engine session callback does race the login model in production, so this matters.
- **Medium 2** `PairingModel.offer`: keeps `.offering` while the new offer opens; `confirmRequest` clears `isWorking`; a stale offer result only closes the window if the user cancelled (state `.idle`), never over a SAS. Test `testSasRequestWhileSwitchingOfferModeIsKept`.
- **Medium 3** `handle(.failed)`: `guard state != .idle, !state.isFinished`. Test `testLateFailureDoesNotOverwriteDoneOrFailed`.
- **Medium 4** new `FsyncMacUI/SignOutButton.swift` (confirmationDialog, Cancel = default action) used by popover, `ReadyView` and Settings (Settings' own dialog removed).
- **Medium 5**: `Window` scene opens at launch. `defaultLaunchBehavior(.suppressed)` could not be used: SceneBuilder rejects `if #available` with the macOS 13 deployment target (compile error), so `AppDelegate.closeLaunchWindow()` closes the "fsync" window right after launch on all versions; onboarding reopens by id. **Not verified**: I built the bundle and launched it (stored fake session + unreachable backend, and signed-out control), but `CGWindowListCopyWindowInfo` and System Events list no FsyncMac windows in either case in this session (no window server access for new windows), so neither "no window for returning user" nor "window opens for signed-out" could be observed. Needs a manual check on a real desktop. Bundle, Keychain items and app support dir were cleaned up.
- **Medium 6**: `TrustResetPromptModel.outcome` is now `public internal(set)` (FsyncCoreKit, one-line hook; its 8 tests pass). `testTrustOutcomeSinkRoutesToPairing` sets `trustModel.outcome` and expects `.needsPairing`, pairing dismissed, devices cleared; fails if the sink is removed.
- **Low 1** `routeToPairing()` shared by `selfRemoved` and the trust outcome; test `testSelfRemovedDismissesPairingAndClearsDevices`.
- **Low 2** all bridge/adapter logs use `privacy: .private`.
- **Low 3** `EngineBridge.live()` fallback uses `AppConfig.fallbackDataDir()` (0700) and sets a banner (`startupNotice`, kept by `applyAccountState`); comment fixed; `createPrivateDir` tightens an existing dir via `setAttributes`. Tests `PrivateDirTests`.
- **Low 4** `loadOrCreateKey` add-only; duplicate re-reads the winner. Test with 6 concurrent callers returns one key.
- **Low 5** `LoginModel.verify`: failed `saveSession` -> best-effort `engine.logout()` via `EngineCall`. No test (a failing real Keychain write cannot be provoked without a mock).
- **Low 6** "Recent items / Nothing yet." removed; `PairOfferView` comment fixed; integration script kills children (`pkill -P`) of swift-test before it, so the xctest child goes too; `StatusSummary.headline` ("Connected" only for lan+relay>0, "Online" for presence only) used by `StatusHeaderView`, unit-tested.

### Q4 answer (late session callback after sign-out)
The core can fire `onSessionChanged` from a token refresh (`service/inner.rs` `on_tokens`), but only while `account` is set; `logout()` calls `set_account(None)`, so no new refresh callback after sign-out. Residual: a `Out::Session` already queued in the dispatcher (or a refresh in flight) when the user signs out. Handled in the app: `SessionGate` (lock-protected flag) is closed by `signOut`/`requireSignIn`, opened by `didSignIn` and by boot with a stored session; `EngineDelegateAdapter.onSessionChanged` ignores tokens while closed. Tests: `testAdapterIgnoresSessionCallbackWhileGateIsClosed`, `testSessionGateFollowsSignInAndSignOut`. `login_verify` emits its own `Out::Session` before `didSignIn`; it is ignored by the closed gate, and `LoginModel` stores those tokens itself.

### Results
- `swift build`: 0 warnings. `swift test` FsyncMac: 86 tests, 3 skipped (integration, no `FSYNC_IT_API`), 0 failures. FsyncCoreKit: 8/8.
- `run-fsync-mac-integration.sh`: PASS (3 tests), twice. Afterwards: no `fsync-mac-it` container, no xctest/fsync-cli processes, no `app.fsync.mac*` Keychain items.
- Docs: `docs/system-architecture.md` (Keychain details), `docs/project-changelog.md`.

### Remaining concerns
- Medium 5 unverified (above). Manual UI checks from round 0 still open; signing/Developer ID question for the DP Keychain still open.
- `requireSignIn` (Keychain delete) and `LoginModel.verify` (save) still touch the Keychain on the main actor; they are fast but would block on a system prompt.

## Round 2 fixes

- **Medium (gate opens too late)**: `EngineBridge+Boot.swift` opens `SessionGate` right after the session read, before `setSession`/`start`; closed again if those calls fail. On cancel it is not touched: the canceller (`signOut`/`requireSignIn` close it, `didSignIn` opens it) owns the gate, and a stale close would race `didSignIn`. Tests: `testSessionGateIsOpenBeforeEngineStart` (uses an internal `beforeEngineStart` seam that runs on the engine thread before `setSession`) and `testGateStaysClosedWithoutStoredSession`. A real mid-`start()` refresh callback cannot be provoked without a live backend, so the test proves the ordering, and the adapter-level store/ignore is covered by the Round 1 adapter test.
- **Low (Code/QR race)**: `PairingModel.offer` returns at once while `isWorking`; the picker is `.disabled(model.isWorking)`. Test `testSecondOfferWhileOneIsRunningIsIgnored`.
- **Low (Keychain on main actor)**: `LoginModel.verify` saves via `Task.detached` and awaits it, so the session is stored before `onSignedIn`/phase change (failure path still logs out). `requireSignIn` clears the session in a detached task chained after the previous clear (`sessionClear`); `signOut` and `shutdown` await it, and the gate is closed first so nothing can re-store a token meanwhile. Residual: a clear still pending when a new sign-in saves could delete the new session; it is milliseconds against a human typing an email and OTP, so no extra coupling was added.
- Results: FsyncMac 89 tests, 3 skipped, 0 failures, 0 warnings; FsyncCoreKit 8/8; integration PASS (3 tests), cleaned up (no container, xctest, fsync-cli or `app.fsync.mac*` Keychain items). `mystifying_euclid` untouched (still Up). Low 3 (launch window) and Low 4 stay as noted open items.
