# Phase 02: Onboarding (email, OTP, first device)

## Context Links
- docs/ffi-contract.md (login_start, login_verify, ensure_account)
- core/fsync-cli/src/cmd_account.rs `login()` (reference order)
- Phase 01 (EngineBridge, AppSecrets, ErrorText)

## Overview
Priority P1 · Status completed · 5h. Sign-in through the email OTP, then the account state decides the next screen: Created shows the "first device" screen, Ready goes to the menu, and NeedsPairing goes to the phase 3 pairing.

## Key Insights
- `login_verify` returns `SessionTokens` and sets the session in the core. The app saves the tokens immediately; core refreshes later arrive via `on_session_changed`.
- `ensure_account` creates the genesis list when the account has none, so Created means "first device". No extra call is needed.
- OTP mistakes come back as `Invalid`/`Auth` and must stay on the code screen; `Network` gets a retry option.

## Requirements
- LoginModel steps: `.email` → `.code(email)` → done.
  - `sendCode()` is enabled only for a plausible email: it is trimmed, contains "@" and is at most 254 characters.
  - The code field accepts digits only and is the backend length (6).
  - "Resend code" goes through `loginStart` again, with a 30 s cooldown in the UI.
  - "Use another email" returns to `.email`.
- Busy flag blocks double submits. Errors are shown inline via `ErrorText` (context `.login`).
- After verify:
  1. `AppSecrets.saveSession(tokens)`.
  2. `bridge.didSignIn()`, which runs `start` (if not started) → `ensureAccount`.
  3. Set the phase; `isFirstDevice = (state == .created)`.
- FirstDeviceView: "This Mac is your first device. Add others from their app with a pairing code." Button "Done" leads to the menu.

## Architecture
```
LoginView/OtpView (Window "main") → LoginModel (@MainActor, engine: FsyncEngineProtocol,
   onSignedIn: (SessionTokens) async -> Void)  → EngineCall.run { loginStart / loginVerify }
   → EngineBridge.didSignIn(tokens) → AppSecrets + ensureAccount → AppPhase
```
LoginModel does not know EngineBridge (it takes a closure), so it is testable with a real engine alone.

## Related Code Files
Create:
- `Sources/FsyncMacCore/Features/Onboarding/LoginModel.swift`
- `Sources/FsyncMacUI/Onboarding/LoginView.swift`
- `Sources/FsyncMacUI/Onboarding/OtpView.swift`
- `Sources/FsyncMacUI/Onboarding/FirstDeviceView.swift`
- `Tests/FsyncMacCoreTests/LoginModelTests.swift`

Modify:
- `Sources/FsyncMacCore/Bridge/EngineBridge.swift`: add `didSignIn`, `isFirstDevice`
- `Sources/FsyncMacUI/RootView.swift`: route `.signedOut` and first-device

## FFI calls
`loginStart(email:)`, `loginVerify(email:code:)`, `start`, `ensureAccount`.

## Implementation Steps
1. LoginModel with validation helpers (pure static funcs, unit tested).
2. Views: the email field is the default focus and Return submits. The OTP field uses `.textContentType(.oneTimeCode)` and auto-submits at 6 digits.
3. `EngineBridge.didSignIn`, so the sign-in state survives a relaunch through the stored session.
4. Menubar popover when signed out: "Sign in…" opens Window "main".
5. Tests:
   - Invalid emails are rejected without an engine call.
   - `sendCode` against `127.0.0.1:9` → Network text, not busy, step stays `.email`.
   - `verify` with a wrong code offline → error, no session saved (check the test Keychain is empty).

## Todo List
- [x] LoginModel + validation tests
- [x] LoginView / OtpView / FirstDeviceView
- [x] Bridge didSignIn + RootView routing
- [x] Offline error-path tests
- [x] Extra: "Lost your other devices?" recovery (AccountRecoveryModel, RecoverySheet, two-step confirm, tests)

## Success Criteria
- Unit tests green. The happy path is asserted by the phase 6 integration test (OTP from the backend log → Created → `.ready`, `isFirstDevice`).

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| TextField focus in an LSUIElement app | M×M | Use Window scene + `NSApp.activate`; never edit text in the MenuBarExtra |
| OTP brute force | L×M | Rate limits are server-side. The client cooldown is UX only. |

## Security Considerations
- The email and code are never logged in clear (`privacy: .private`). Tokens go only to the Keychain.
- The code field is cleared after an error and after success.

## Next Steps
Phase 3 handles `.needsPairing`.
