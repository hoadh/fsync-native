# Report: macOS app shell phases 3-4 (pairing, menu, devices, trust reset)

## Result
In apps/macos/FsyncMac, `swift build` is clean (0 warnings after a forced full rebuild). `swift test`: 59 tests, 0 failures (55 Core, 4 UI render). That is 32 more than phases 1-2 (27). FsyncCoreKit `swift test`: 8/8. Largest new source file: PairingModel.swift, 172 lines.

## Answer: does `logout()` clear the in-memory session?
Only partly (core/fsync-core `service/login.rs`, `api/lists.rs`).
- `api.logout()` posts to the server first. It clears the in-memory tokens only after the server call succeeds.
- If the server is unreachable or there are no tokens, the tokens stay in memory.
- The service still runs `set_account(None)`, drops presence and clears the trust pin, so the engine acts signed out.
- The app does not depend on this. `signOut()` calls `logout()` for best effort, then always runs `requireSignIn`: it clears the Keychain session, resets the login, devices and pairing models, and goes to `.signedOut`.
- Test: `testSignOutClearsKeychainEvenWhenServerIsUnreachable`.
- Remaining gap: after an offline sign-out the old refresh token is still valid server-side and stays in engine memory until the next `loginVerify` replaces it. Fixing that needs a core change (revoke or clear on failure).
- Sign out is a button in the main-window home and in the popover.

## Files (apps/macos/FsyncMac/)
- Package.swift: added the `TrustResetPrompt` product to Core, UI and UITests.
- Core:
  - Features/Pairing: `PairingModel`, `PairingCode`.
  - Features/Devices: `DevicesModel`, `StatusSummary`, `PeerText`.
  - Bridge: new `EngineBridge+Models`; `EngineBridge`, `+Boot` and `+Events` rewritten or extended.
- UI:
  - Pairing: `PairingFlowView`, `PairOfferView`, `PairJoinView`, `SasConfirmView`, `QrCodeImage`.
  - Devices: `DeviceListView`, `DeviceRowView`.
  - Menu: `MenuBarContentView`, `StatusHeaderView`.
  - Also `ReadyView` (main-window home with remove confirm), `TrustGate`; `RootView`, `MenuPopover` and `NeedsPairingView` modified.
- App: `FsyncMacApp` now has the filled/outline icon, window auto-open and the window-request hook.
- Tests:
  - New: `PairingModelTests` (incl. helpers), `DevicesModelTests` (incl. StatusSummary), `EngineBridgeEventsTests`.
  - `OnboardingRenderTests` extended: all pairing states, QR, devices, menu, ready home.
- Plan: phase 3 and 4 todos ticked and statuses set to completed in the phase files and plan.md.

## Behaviour implemented
- **Pairing**
  - The SAS words, peer name and platform are shown verbatim, sanitized, plus the first 8 characters of the node id.
  - "They don't match" is the default button; there is no auto-accept.
  - `confirm(requestId:accept:)` only acts on the id that was displayed.
  - A newer `ConfirmRequest` replaces the peer, and the generation counter drops an answer still in flight. This is tested.
  - Offer: a code in groups of four, a copy button, a countdown, "Get a new code", and a Code/QR switch (cancel then re-offer).
  - Join: a code field, a connecting spinner with elapsed time, and Cancel.
- **needsPairing**
  - The offer starts automatically.
  - The recovery link stays below it.
  - After `Completed`, the offering side calls `ensureAccount` and goes to Ready. The "Paired with X" screen stays until the user taps Done.
- **Account events**
  - `.removedNeedsPairing` and `selfRemoved` go to `.needsPairing`.
  - `sessionExpired` and Auth errors go to sign-in.
  - `historyReset` shows a banner.
  - Other errors show a banner through ErrorText.
- **Trust reset**
  - `AccountEvent.trustReset` is forwarded to `TrustResetPromptModel.handle` on the main actor, and `windowRequest` opens the main window.
  - `TrustGate` shows `TrustResetPromptView` whenever `pending` is non-empty or the outcome is not `.unanswered`.
  - `refresh()` runs after each successful `ensureAccount`.
- **Devices**
  - Sorted with self first.
  - Status events are merged by node id.
  - The list reloads on `listAdvanced`, popover open, wake and after pairing.
  - Remove uses a `confirmationDialog` in the main window. Cancel is the default and the dialog is bound to the node id shown. Self has no remove action.
  - The popover is read-only and shows a "Review…" link while a trust reset is pending.
- **First launch**
  - The main window opens once when the phase first becomes signedOut, needsPairing or bootFailed.
  - Returning Ready users get no window.

## Deviations
- Added `PeerText.sanitize` (drops control and bidi characters, caps at 40 characters). All untrusted names and reasons go through it.
- `PairingModel` has `isJoinOpen`, `isActive` and `openJoin()` for the "Add device…" entry. `confirm`, `cancel` and `join` are async rather than fire-and-forget.
- Offer with no account returns `Invalid` from the engine, not `Auth`. It shows `.failed` with the general text, because "Code invalid" would mislead on an offer. Join keeps the pairing text. A real `Auth` still routes to sign-in.
- `removeDevice` with no account also returns `Invalid`, so it shows text and does not go to sign-in. The test documents this.
- "Settings…" is not in the popover. Phase 5 owns it.
- The trust outcome sink is in `handleTrustOutcome(_:)`, so tests can call it. `TrustResetPromptModel.outcome` is not settable from outside its package.
- Window auto-open uses the menubar label's `onReceive` plus `makeKeyAndOrderFront`. An accessory app's new window can otherwise stay behind other apps.
- A `Window` scene is shown at launch by default on macOS 13, with no way to suppress it. The launch behaviour also depends on state restoration.
- `PairJoinView` is not render-tested (it uses `@FocusState`; same limit as LoginView).

## Smoke
I launched the debug binary against dev defaults. With a clean Keychain and data dir the phase reached signedOut and `showMain` ran. The "fsync" window reports `isVisible` true, but CGWindowList and occlusion did not list it as on screen in this tool session. I could not check it visually.
Rebuilding the unsigned binary makes the file Keychain wait on an ACL prompt that nobody can answer, and boot then sits at "Starting". That happened twice during my runs. I cleaned the Keychain after each run. The items `app.fsync.mac` (device_secret, history_key) and `~/Library/Application Support/app.fsync.mac` are deleted. I checked that none remain.

## Not verified
- The remove and trust-reset `confirmationDialog` rendering and the real SAS flow are manual or phase 6 (the Phase 4 manual todo stays unticked).
- The happy paths that need a backend are untested here: `Completed` against a real peer, a trust-reset confirm, and a device removal that succeeds.

## Unresolved questions
1. Should the core clear in-memory tokens when `logout` fails over the network (or revoke the refresh token later)? It is a core change, so I left it.
2. The pairing code is copied to the general pasteboard. Once clipboard sync (phase 12) exists, this code could sync to other devices. Should the pasteboard write be marked concealed or transient (`org.nspasteboard.ConcealedType`)?
3. Should `expires_ms` be treated as Unix ms from the backend for code mode? It is for QR mode (`now_ms()`); the code-mode value comes from the server's `expires_at`.
4. For the first-launch window, does the owner want a more visible placement than "ordered in when not ready"?

**Status:** DONE_WITH_CONCERNS
**Summary:** Phases 3 and 4 are implemented and tested: pairing in both roles with an id-bound SAS check, the menu, device list, live status, remove with confirm, and trust-reset wiring. Build has 0 warnings and 59 FsyncMac tests plus 8 FsyncCoreKit tests pass. Plan files are ticked.
**Concerns/Blockers:** Dialog rendering and window visibility could not be checked visually. After a failed network logout the core keeps the old refresh token in memory, though the app clears the Keychain and signs out regardless. Backend happy paths wait for phase 6.
