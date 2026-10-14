## Round 1

**Score: 9.0 / 10.** Gate is >=9.5 with 0 Critical, so this fails on score. Critical 0, High 1, Medium 6, Low 6.

Verified: `swift build` OK, 0 warnings. `swift test` 68 tests, 0 failures, 3 skipped (the integration tests). `run-fsync-mac-integration.sh` PASS (3/3). It removed its container and its Keychain items, and `mystifying_euclid` was not touched. All files are under 200 lines (max 172). The docs match the code: test counts, the integration claim and the Keychain fallback text.

### Critical
None.

### High
1. **Stale boot task can overwrite newer state.** `EngineBridge+Boot.swift:26` and `:78` call `fail(error)` without checking `Task.isCancelled`. Only the success path (`:71`) checks it. `signOut`, `didSignIn` and `requireSignIn` cancel `bootTask`, but an `ensureAccount` call that is already running still returns afterwards. A non-network failure then sets `.bootFailed` over `.signedOut` or over the new boot. An `Auth` failure calls `requireSignIn()` again and sets the "Sign in again." banner after a deliberate sign-out. A `Network` failure sets `banner` on the sign-in screen (`:79`).
   - Fix: after each `await EngineCall.run` in `runBoot` and `resolveAccount`, add `guard !Task.isCancelled else { return }` before any `fail`, `banner =` or `phase =`.
   - Add a test: start `didSignIn` against the offline config, call `signOut`, and assert the phase stays `.signedOut` with a nil banner.

### Medium
1. **Keychain store choice has no migration, and the probe can misreport.** `KeychainStore.swift:25-32`.
   - The data-protection probe treats `errSecDuplicateItem` as "unsupported". That can happen after a crash between the probe's add and delete, and the fallback then sticks for the process.
   - When a signed build later gets the data-protection keychain (the roadmap lists this as open), `get` finds nothing. `loadOrCreateKey` then silently makes a new `device_secret` and `history_key`. The result is a new node identity and `HistoryKeyMismatch`, which pushes the user to "Start fresh".
   - Fix: treat `errSecDuplicateItem` as success in the probe. In data-protection mode, if `get` finds nothing, read the file keychain and copy the item over. At minimum, document the trap in the Keychain section of `system-architecture.md`.
   - Missing test: nothing covers the fallback or the probe.
2. **A SAS `confirmRequest` is dropped in some states.** `PairingModel.swift:119-129` handles it only in `.offering`, `.waitingForPeer` and `.sas`.
   - While the user switches Code/QR in `PairOfferView`, state is `.idle` with `isWorking` true (`offer()` runs `begin(.idle)`). A peer request that arrives then is never shown and never answered, so the flow hangs until the offer expires.
   - Fix: also accept the request while `isWorking` and state is `.idle`, or have the offer switch keep `.offering` state until the new offer lands.
3. **A late `.failed` event overwrites `.done`.** `PairingModel.swift:135-140` guards only `state != .idle`. After "Paired with X" is shown and `onPaired` has run, a trailing failure turns the screen into "Pairing didn't finish".
   - Fix: `guard state != .idle, !state.isFinished`.
4. **Sign out has no confirmation in two places.** `MenuBarContentView.swift:31` and `ReadyView.swift:34` call `signOut` directly. Settings (`SettingsView.swift:61-66`) asks first. In the popover the button sits next to "Quit". Remove, reset and the other destructive actions do confirm.
   - Fix: reuse one confirm (a shared helper or modifier), or send these buttons to Settings.
5. **First-launch window may open for a returning user (unverified, no screen available).** `FsyncMacApp.swift:32` uses a `Window` scene. On macOS 13 and 14 a `Window` scene opens at launch, and `.defaultLaunchBehavior(.suppressed)` needs macOS 15. That would make the `autoOpened` logic and the "returning user needs no window" comment wrong. The dev reports list this as an unchecked manual item.
   - Fix: check on a screen. If it opens, close it in `applicationDidFinishLaunching`, or gate it with `#available(macOS 15)` plus `.defaultLaunchBehavior(.suppressed)`.
6. **The trust-reset sink is not tested.** `EngineBridgeEventsTests.swift:60-70` calls `handleTrustOutcome` directly. The real wiring (`EngineBridge+Models.swift:22`, the `trust.$outcome` sink) is never exercised. Dropping that sink would not fail any test.
   - Fix: drive it through the model. A helper in `TrustResetPrompt` that sets `outcome`, or an integration step with a pending reset, would do.

### Low
1. **Inconsistent routing to pairing.** `EngineBridge+Events.swift:25-28` (`selfRemoved`) sets `.needsPairing` without `pairingModel?.dismiss()`. `handleTrustOutcome` (`EngineBridge+Models.swift:31-36`) does dismiss. A join screen can stay open after the Mac is removed. Fix: use one `routeToPairing()` for both.
2. **Engine errors are logged with `privacy: .public`.** `EngineBridge.swift:73` and `:112` log `String(describing: error)`. The engine error text can carry server detail. `ErrorText` already uses `.private`. Fix: use `.private` for the described error and keep the code or case name public.
3. **Temp-dir fallback is silent and does not match its comment.** `EngineBridge.swift:50-55` says "the user is told nothing is kept", but nothing tells the user. Secrets stay in the Keychain while the history is lost on reboot, and the directory is not created with 0700. Fix: show a banner and create the directory with 0700, or make the failure a `.bootFailed`. Separately, `AppConfig.defaultDataDir` (`AppConfig.swift:50`) does not tighten the permissions of a directory that already exists. Fix: call `setAttributes` after creating it.
4. **`loadOrCreateKey` overwrites on a race.** `AppSecrets.swift:25-36` uses an upsert `set`. Two first-time callers could get different keys. It is only called sequentially today. Fix: add without update, and on a duplicate re-`get`.
5. **After a failed `saveSession` the engine still holds a live session.** `LoginModel.swift:101-104` tells the user to request a new code, but the engine stays signed in with no stored tokens. Fix: call `logout()` best-effort in that branch.
6. **YAGNI and small polish.**
   - The "Recent items / Nothing yet." placeholder in `MenuBarContentView.swift:25-26` is dead UI until phase 12.
   - The comment at `PairOfferView.swift:30` ("the model ignores nothing") is garbled.
   - `run-fsync-mac-integration.sh:112` kills `swift test` but not its `xctest` child. `pkill -f "$WORK"` does not match it, so an interrupted run can leave an orphan.
   - `StatusHeaderView.swift:9` says "Connected" when the only online peers are presence-only (not yet connected).

### Checked and fine
- **Secrets and logs:**
  - Secrets are only in the Keychain (`AppSecrets`). Tokens are saved in the engine callback so a crash cannot lose a refresh.
  - A corrupt key is not replaced. Unreadable session JSON is deleted and counts as signed out.
  - `ErrorText` never shows engine details in the UI.
  - Both Keychain stores get `AfterFirstUnlockThisDeviceOnly`.
- **Pairing:**
  - `confirm` checks `peer.requestId`. The view captures the on-screen id and carries `.id(peer.requestId)`. "They don't match" is the default button.
  - `generation` drops stale results.
  - The `Task.detached` captures are `Sendable`.
  - A confirm that is already running is dropped when a newer request arrives (tested).
- **Peer text and pasteboard:** `PeerText.sanitize` strips control and format characters, so bidi overrides are removed. Names are shown with `Text(verbatim:)`. Concealed and transient markers are set and tested.
- **Callbacks and guards:** Callbacks use `DispatchQueue.main.async` with `MainActor.assumeIsolated`, which keeps event order. `isBusy` guards the double-tap cases (`remove`, recovery, `answer`). Recovery takes two steps, and Cancel is the default button.
- **Keychain duplicate race and AccountState mapping:**
  - The duplicate-item race is handled in `set` and covered by `testConcurrentFirstWritesDoNotFail` (real Keychain, real threads).
  - `Created` and `Ready` map to `.ready`. A missing session goes to `.signedOut`.
- **Status display:** "Online" is shown when online but status is offline (`DeviceRowView`, `StatusSummary`, with a test).
- **Trust-reset wiring:** `TrustResetPromptView` is wired with `onDone`, which runs `dismissOutcome` inside the library. So `.removedNeedsPairing` routes to `.needsPairing` and the outcome screen stays until Done.
- **Scripts:**
  - Quoting and `set -euo pipefail` are good, and the EXIT/INT/TERM traps return the right codes.
  - The container name is unique (`$$`) and removed with `-v`. The port is bound to localhost.
  - Keychain cleanup matches only the `app.fsync.mac.tests.` prefix.
  - The build script validates the bundle id, and the plist values are passed as arguments, so there is no injection.
  - The tests are real: real engine, real Keychain, real backend, no mocks.

### Unresolved questions
1. Does the `Window` scene open at launch on macOS 13 and 14 (Medium 5)? It needs a visual check.
2. Is a Developer ID or provisioning profile planned? That decides how much the Keychain migration (Medium 1) matters, and it is the same question as plan question 2.
3. Should sign out from the popover go to Settings, or confirm in place (Medium 4)?
4. Does the core keep refreshing tokens after an offline `logout()` (the known gap in the changelog)? If it does, `onSessionChanged` could re-store a session after sign-out. This looks unlikely, since nothing authenticated runs while signed out, but I did not confirm it.

To reach 9.5, fix High 1 and Medium 1-4 and add their tests.

## Round 2

**Score: 9.5 / 10. Gate met (>=9.5, 0 Critical).** Critical 0, High 0, Medium 1, Low 4.

Verified: FsyncMac `swift build` 0 warnings and `swift test` 86 tests, 3 skipped, 0 failures. FsyncCoreKit `swift test` 8/8. I did not re-run the integration script. Largest source file is 179 lines.

### Round 1 fixes, checked in code
- **High 1 (stale boot task): fixed.** `EngineBridge+Boot.swift:19`, `:34`, `:68` and `:78` check `Task.isCancelled` before any `fail`, `banner` or `phase` write. `started` is still recorded (`:32`). A test covers sign-out during the account check.
- **Medium 1 (Keychain migration and probe): fixed.**
  - `KeychainStore.swift:46-48` treats `errSecDuplicateItem` as supported.
  - `readMigrating` (`:88-101`) keeps the data when the copy fails and retries on the next read.
  - `delete` removes both copies (`:145`).
  - The real data-protection path cannot run unsigned, so it is tested only through the injected closures.
- **Process-wide lock: sound.** `NSRecursiveLock` makes `get` -> `set` -> `add` safe to re-enter. The probe at `:35-43` runs once, outside the lock, which is fine. The concurrent-writer test now finishes in seconds.
- **Medium 2 (SAS dropped while switching offer mode): fixed.** `PairingModel.swift:55-75` keeps `.offering` while the new offer opens. A stale result closes the window only when state is `.idle`.
- **Medium 3 (late `.failed` over `.done`): fixed.**
- **Medium 4 (sign-out confirm): fixed.** `SignOutButton` is used in the popover, `ReadyView` and Settings, with Cancel as the default answer.
- **Medium 6 (trust-outcome sink): fixed.** `outcome` is now `internal(set)` in FsyncCoreKit, a one-line change, and a test drives the real sink.
- **Low 1-6: fixed.**
  - `routeToPairing` is shared by the `selfRemoved` and trust-outcome paths.
  - Logs use `.private`.
  - The temp-dir fallback is 0700 with a banner.
  - `loadOrCreateKey` is add-only.
  - A failed `saveSession` calls `logout()` best-effort.
  - The script kills the children of `swift test` with `pkill -P`.
- **SessionGate: sound.** It is closed in `signOut` (before `logout`) and in `requireSignIn`, and opened in `didSignIn` and boot. The adapter ignores tokens while it is closed (`EngineDelegateAdapter.swift:46`), which answers Q4 from Round 1.

### Medium
1. **SessionGate opens too late in boot.** `EngineBridge+Boot.swift:36` opens the gate only after `setSession` and `start` have returned (`:28-31`). `start()` can begin authenticated work, and the first call after a long gap refreshes an expired access token. That refresh callback is then dropped. If the backend rotates refresh tokens (migration `0002_refresh_session` suggests it does), the stored refresh token is stale, and the next launch forces a re-login.
   - Fix: call `sessionGate.set(open: stored != nil)` right after the session read (before the `EngineCall`), and close it again if that call fails.
   - Add a test: a callback that arrives during `start` is stored.

### Low
1. **Main-thread Keychain calls now share one lock.** `requireSignIn` (`EngineBridge.swift:~84`) and `LoginModel.verify` (`:101`) call the Keychain on the main actor. The new lock means that if the engine's session callback is waiting on a system prompt (for example after an ad-hoc rebuild changes the ACL), the main thread waits too. Prompts normally show on the first access, which is now the off-main boot read, so it is narrow. Move both calls to `Task.detached` as a follow-up.
2. **Rapid Code/QR taps can race at the core.** The picker in `PairOfferView` still allows a new `offer()` while one is running. Two `pairingOffer` calls can run in either order, so the core may end up with the older secret while the UI shows the newer code. Fix: ignore picker changes while `isWorking`.
3. **Launch-window close is timing-dependent.** `AppDelegate.swift:21-25` closes the "fsync" window on the next main-queue turn. If SwiftUI orders the window in later than that, it stays open. If boot finished first, the close would also hide a window that onboarding had just opened. Both are plausible, and it is cosmetic only (a returning user sees one extra window, or an onboarding window flashes). Keep this as an open manual check. If it misbehaves, close the window in `RootView.onAppear` when the phase is `.booting` or `.ready`.
4. **The FsyncCoreKit change reaches past the library's API.** Loosening `outcome` to `internal(set)` only helps a test that uses `@testable`. It is harmless, but give the library a `setOutcomeForTesting`-style hook, or keep the sink under test, if more tests need it.

### Do the developer's open items block the gate?
- **`requireSignIn` and `LoginModel.verify` touch the Keychain on the main actor: does not block.** It is Low 1, a short call with a narrow trigger. Track it as a follow-up.
- **No test for the saveSession-failure logout: does not block.** The code path is three lines, best-effort, and covered by the existing `EngineCall` pattern. If wanted, inject a failing store closure, as the migration code already does.
- **Medium 5 not visually verified: does not block** the code-review gate, because the worst case is cosmetic (Low 3). It must stay on the manual checklist, and the phase should remain "Done with concerns", as the roadmap already says.

### Unresolved questions
1. Does the backend rotate refresh tokens on every refresh? That decides how bad Medium 1 is (a forced re-login versus no effect).
2. Does the launch window close reliably on macOS 13, 14 and 15? It needs a manual check on a real desktop.
3. Is a Developer ID build planned for phase 12? That is when the migration code first runs against the real data-protection store, so it should be exercised once on a signed build.

## Round 3

**Score: 9.6 / 10. Gate met.** Critical 0, High 0, Medium 0, Low 2 (new, optional).

Verified: FsyncMac `swift build` 0 warnings; `swift test` 89 tests, 3 skipped, 0 failures. FsyncCoreKit 8/8. I did not re-run the integration script.

### Round 2 fixes, checked
- **Medium (gate opens too late): fixed.**
  - `EngineBridge+Boot.swift:~29` opens the gate right after the session read and before `setSession` and `start`. The `beforeEngineStart` hook runs inside the engine call, so the test proves the ordering.
  - The gate is closed again on a non-cancelled failure.
  - A cancelled boot leaves the gate alone, and the canceller owns it. That is correct: a stale close could otherwise race `didSignIn`. The `guard !Task.isCancelled` before the failure branch makes this hold.
- **Offer re-entry guard: fixed.** `PairingModel.swift:58` returns early while `isWorking`. `isWorking` is cleared on success, failure, SAS request and cancel, so the guard cannot get stuck. The picker is disabled while `isWorking`.
- **Keychain off the main actor: fixed, ordering kept.**
  - `LoginModel.verify` stores the session in a detached task and awaits it, so the session is in the Keychain before `onSignedIn` changes the phase. The failure path still logs out.
  - `requireSignIn` chains clears through `sessionClear`. `signOut` and `shutdown` await it, and the gate is closed before the clear is queued.

### The pending-clear versus new-sign-in race
It cannot delete a fresh session in practice, and it does not block the gate.
- The clear task is created in `requireSignIn` and starts right away. A fresh save needs the user to type an email, wait for the mail and enter the code, which is seconds to minutes. The clear (a few Keychain calls) finishes long before.
- The process-wide lock does not order them by arrival (`NSRecursiveLock` is not FIFO). It orders them only in the sense that the two never overlap.
- A bad order needs the clear to be stuck behind a blocked call (a system prompt held under the same lock) for longer than a human sign-in, and then lose the lock race to the save. The result would be a fresh in-memory session with no stored copy, so the user signs in again on next launch. No data is lost and no secret leaks.
- A stale `sessionExpired` or `Auth` event that arrives after a fresh sign-in also clears the session. That is the intended behaviour (the session really was rejected), not this race.

### Low
1. **Optional hardening for the race above.** Make `LoginModel.verify` wait for any pending clear before saving: pass the bridge's `sessionClear` in as `beforeSave: () async -> Void`, or an equivalent. That makes the order strict at almost no cost.
2. **Boot can read a session that is about to be cleared.** `runBoot` (`EngineBridge+Boot.swift:~18`) does not await `sessionClear`. After `requireSignIn` followed quickly by `startFresh` or `boot()`, it can read the old session, open the gate, and hit `Auth` again. It ends in the right state (signed out) but costs one extra round trip. Fix: `await sessionClear?.value` before the read.

### Still open (non-blocking, unchanged)
- The launch-window close (Round 2 Low 3) and the `internal(set)` library hook (Round 2 Low 4) are accepted open items.
- The manual UI checklist and a signed-build check of the Keychain migration remain for phase 12.

### Unresolved questions
1. Does the backend rotate refresh tokens? This still sets how much the gate ordering matters. The fix is correct either way.
