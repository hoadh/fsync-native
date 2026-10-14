# Report: macOS app shell phases 1-2 (+ recovery path)

## Result
`swift build && swift test` in apps/macos/FsyncMac: clean rebuild, 0 warnings, 27 tests, 0 failures (26 Core, 1 UI render). FsyncCoreKit `swift test`: 8/8 pass. Smoke: `.build/debug/FsyncMac` launched and stayed alive 5 s, then I killed it. Dev Keychain items and data dir that the smoke run created were deleted.

## Keychain probe
`SecItemAdd` with `kSecUseDataProtectionKeychain` returns -34018 in an unsigned binary and in an ad-hoc signed one (swiftc spike + `codesign -s -`). The file keychain with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` returns 0. `KeychainStore` probes once per process and falls back to the file keychain. The decision is in the header comment of `KeychainStore.swift`. A signed build with the entitlement would use the data-protection store automatically.

## Files created (all under apps/macos/FsyncMac/)
- `Package.swift`, `.gitignore`. Tools 6.0, macOS 13. Targets: FsyncMacCore, FsyncMacUI, FsyncMac (exec), plus two test targets.
- Core/Bridge: `AppConfig`, `KeychainStore`, `AppSecrets`, `EngineCall`, `AppPhase`, `ErrorText`, `EngineDelegateAdapter` (with `EngineEvent`), `EngineBridge`, `EngineBridge+Boot`, `EngineBridge+Events`.
- Core/Features/Onboarding: `LoginModel`, `AccountRecoveryModel`.
- UI: `RootView` (with Booting and BootFailed views), `MenuPopover`, Onboarding/`LoginView` (holds LoginFlowView), `OtpView`, `FirstDeviceView`, `NeedsPairingView`, `RecoverySheet`.
- App: `FsyncMacApp` (MenuBarExtra `.window` + `Window("main")`), `AppDelegate`.
- Tests (Core): `TestSupport`, `KeychainAndSecretsTests`, `MappingTests` (AppPhase, AppConfig, ErrorText), `EngineBridgeBootTests`, `OnboardingModelTests` (LoginModel), `AccountRecoveryModelTests`.
- Tests (UI): `OnboardingRenderTests`.
- The largest source file is 113 lines.
- Plan edits: ticked the todos in phase-01 and phase-02, marked both phases completed, and set the status column in plan.md to completed.

## Recovery path ("Lost your other devices?")
- `NeedsPairingView` shows the link. It opens `RecoverySheet`, which has two steps: explain, then final confirm. Cancel is the default (Return) button on both steps. The destructive button has `role: .destructive` and no shortcut.
- `AccountRecoveryModel` state machine: idle, explain, finalConfirm. `reset()` calls `resetAccountTrust()` only from finalConfirm.
- The result goes through `EngineBridge.applyAccountState`, the same mapping used by boot and sign-in. `Created` gives `.ready` plus `isFirstDevice`.
- `Auth` and `ReauthRequired` call `bridge.requireSignIn`, which clears the session and shows "Sign in again to continue." Other errors stay in the sheet.
- Tests use a real engine:
  - The destructive call is blocked at idle and explain.
  - With no session, the error routes to sign-in.
  - With a fake session and 127.0.0.1:9, an error message shows and the step stays finalConfirm.
  - The mapping, and the bridge wiring through to `.signedOut` plus the banner.
- The happy path (the engine returning Created) needs a backend, so phase 6 covers it.

## Deviations from the plan
- `didSignIn()` is a sync function with no parameter. `LoginModel` saves the tokens (it takes `AppSecrets`) and calls `onSignedIn: @MainActor () -> Void`, which starts the account check with offline retry.
- `boot()` is sync: it starts a Task, and callers observe `phase`. A stored session with no network never "finishes".
- Extra bridge API: `applyAccountState`, `requireSignIn`, `dismissFirstDevice`, `shutdown`, `startFresh`. `loginModel` and `recoveryModel` are created with the engine.
- `applicationShouldTerminate` returns `.terminateLater`, then calls `shutdown()`. The engine `stop()` and its release run off the main thread.
- `selfRemoved` leads to `.needsPairing`, and `sessionExpired` leads to `.signedOut`, in `receive`.
- Render tests skip `LoginView`/`OtpView`. They use `@FocusState`, and `ImageRenderer` emits a runtime warning for it, which would break the 0-warnings gate. The other onboarding views and both recovery steps are rendered.
- `.textContentType(.emailAddress)` is macOS 14+, so it was dropped. `.oneTimeCode` compiles on 13.
- A boot test deliberately overwrites the stored history key to trigger `HistoryKeyMismatch`. Another test covers `startFresh()`.
- `MenuPopover` was added (not in the phase 2 file list) for "Sign in…" and "Set up this Mac…".

## Unresolved questions
1. Default api base is `http://127.0.0.1:8080` as a guess. Is there a canonical dev port?
2. Nothing opens the main window automatically on first launch. The user has to click the menubar icon, then "Sign in…". Should the app open the window itself?
3. Does the core's `logout()` clear the in-memory session? `requireSignIn` assumes a later `loginVerify` replaces it.
4. Plan unresolved Q2 (signing and a provisioning profile for the data-protection Keychain) is still open.

**Status:** DONE
**Summary:** Phases 1-2 are implemented with the recovery path. The build is clean with 0 warnings, FsyncMac has 27 tests and FsyncCoreKit 8, all passing. The data-protection Keychain is unusable unsigned (-34018), so the file-keychain fallback is in place.
**Concerns/Blockers:** None blocking. Happy-path login and recovery are not exercised until the phase 6 backend integration test. See the deviations and open questions above.
