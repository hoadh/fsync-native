# macOS App Shell: Pairing, Recovery, and a Keychain Race the Integration Test Caught

**Date**: 2026-10-14 15:00
**Severity**: High
**Component**: apps/macos/FsyncMac (Core, UI, scripts), TrustResetPrompt wiring from FsyncCoreKit
**Status**: Resolved for shell scope. Manual UI checks, the signing decision, and the offline-logout token are still open.

## What Happened

Built the macOS menubar shell (phases 1-6) as a SwiftPM package. `scripts/build-fsync-mac-app.sh` assembles the `.app`. The shell covers OTP login, first device, "Lost your other devices?" recovery, two-way pairing with SAS, device list and remove, trust-reset wiring, settings, and launch at login. `scripts/run-fsync-mac-integration.sh` runs three tests against a real dev backend, Postgres in Docker, with a real engine and a real CLI peer. Review went 9.0, 9.5, 9.6 (gate 9.5, zero Critical). Final gates: FsyncMac 89 tests, 3 skipped, 0 failed, 0 warnings. FsyncCoreKit 8 + 14 pass. Integration 3/3 pass in about 15 s. cargo 253 passed, 1 ignored.

## The Brutal Truth

The worst bug was in code I had already called done, and the unit tests never exercised it. Login saved the session, and the engine's `onSessionChanged` saved it again. Both saw "not found" and both called `SecItemAdd`. One got `errSecDuplicateItem`, the user saw "Couldn't store your sign-in securely," and they were signed out. Separately, concurrent first adds to the file keychain deadlocked. One concurrent-writer test hung for 16 minutes before `sample` showed where.

## Technical Details

- `KeychainStore.set` retries as an update on `errSecDuplicateItem`. Without the fix, 3 of 4 concurrent writers failed.
- One process-wide `NSRecursiveLock` now covers all `KeychainStore` calls.
- A Keychain read in `runBoot` froze the main thread during bundle testing. It now runs off the main actor.
- A stale boot task could set `.bootFailed` or "Sign in again." after sign-out. Every fail, banner, and phase write is guarded by `!Task.isCancelled`. Test: `testSignOutDuringAccountCheckLeavesSignedOutAndNoBanner`.
- `SessionGate` opened only after `start()` returned, so a refresh during start was dropped. The gate now opens after the session read and before `setSession`/`start`. Whether the backend rotates refresh tokens is unconfirmed. If it does, the stored token goes stale and the user re-logs in.

## Decisions

- **SwiftPM, not xcodeproj.** Build and test run from the command line. The bundle is a script output.
- **Text entry stays in the main window.** The popover only launches flows and is read-only for devices.
- **SAS default is "They don't match."** There is no auto-accept, and `confirm` acts only on the displayed `requestId`.
- **Pairing code is marked `org.nspasteboard.ConcealedType` and `TransientType`,** so clipboard managers and future sync skip it.
- **`SessionGate`** closes on sign-out and requireSignIn, opens on didSignIn and on boot with a stored session. The adapter drops tokens while closed, so a late refresh cannot re-store a session after sign-out.
- **Keychain falls back to the file store when unsigned.** The data-protection store returns -34018 even ad-hoc signed. The migration path is tested only through injected closures.

## Deviations

- Status shows **Online**, not Lan/Relay, until phase 12. The core reports Lan/Relay only for live QUIC, and nothing connects yet. The integration test asserts `online` within 5 s (measured 0.3 s), not `.lan`.
- Offline sign-out leaves the old refresh token in engine memory until the next login. The app clears the Keychain anyway, so this needs a core change.

## Lessons Learned

- Two writers to one secret need one owner or a write that tolerates conflict. "Both sides save" is a bug waiting for an integration run.
- Every async completion must check cancellation before it writes state.
- `ImageRenderer` cannot render `@FocusState` views without warnings, which break the zero-warning gate. `LoginView`, `OtpView`, and `PairJoinView` have no render coverage.
- Green tests are not visual verification. Nobody has seen the menubar focus, the dialogs, or the Settings window on screen.

## Next Steps

- **Manual checks** (owner: whoever has a desktop, no date): MenuBarExtra focus, remove and trust-reset dialogs, Settings window, launch-at-login toggle, sign-in from the bundle, and whether the launch window opens for returning users.
- **Developer ID decision** (owner: product, before phase 12). It decides when the migration runs against a real data-protection store.
- **Offline-logout token** (owner: core). Clear or revoke the refresh token when logout fails.
- **Integration tests are not in CI** as far as the reports show. They run only through the script. Wire them into CI or accept manual-only (owner: unassigned).

**Status:** DONE_WITH_CONCERNS
**Summary:** The macOS shell (phases 1-6) is built and reviewed to 9.6. Tests pass across FsyncMac, FsyncCoreKit, cargo, and the 3/3 integration run. Four real races were fixed, including the Keychain duplicate-add found by the integration run.
**Concerns/Blockers:** No manual visual checks yet. The Developer ID decision is open. The offline-logout refresh token needs a core change. Integration tests are not in CI.
