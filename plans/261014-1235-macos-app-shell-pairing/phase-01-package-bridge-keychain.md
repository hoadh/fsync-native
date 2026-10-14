# Phase 01: Package, EngineBridge, Keychain, boot

## Context Links
- docs/ffi-contract.md (threading rules, EngineConfig, HistoryKeyMismatch, reset_history)
- apps/macos/Packages/FsyncCoreKit/Sources/TrustResetPrompt/TrustResetPromptModel.swift (pattern)
- apps/macos/Packages/FsyncCoreKit/Tests/FsyncCoreKitTests/FsyncCoreKitTests.swift (real-engine test setup)

## Overview
Priority P1 · Status completed · 10h. Create the package skeleton, secrets storage, the bridge and the boot state machine. At the end of this phase the menubar app launches, boots the engine from the Keychain, and shows a phase placeholder.

## Key Insights
- Every `FsyncEngine` method blocks, and callbacks arrive on the "fsync-callbacks" thread. Generated records and `FsyncEngineProtocol` are `Sendable`.
- Event order matters (for example ConfirmRequest before Completed). Forward with `DispatchQueue.main.async { MainActor.assumeIsolated { … } }`, which is FIFO. A `Task { @MainActor }` per event does not guarantee order.
- `on_session_changed` must persist the tokens. It writes to the Keychain directly on the callback thread (the Keychain is thread-safe), so a crash right after a refresh cannot lose them.
- The core persists no session. CLI order is `set_session` (if stored) → `start()` → `ensure_account()`.
- An ad-hoc signed app cannot use the data-protection keychain (`errSecMissingEntitlement` -34018).
- A lost Keychain with the data dir still present gives `HistoryKeyMismatch` at init. The app must not wipe the history silently.

## Requirements
- Functional:
  - The device_secret and history_key are 32 random bytes from `SecRandomCopyBytes`, generated once and loaded on every launch.
  - Tokens are stored as JSON in one Keychain item.
  - The data dir is `~/Library/Application Support/app.fsync.mac/` (0700).
  - The config comes from Info.plist keys `FsyncApiBase`/`FsyncRelayUrl`, overridden by the `FSYNC_API`/`FSYNC_RELAY` env vars (dev).
  - `allowInsecureHttp` is true only for http on `127.0.0.1` or `localhost`.
  - Device name: `Host.current().localizedName ?? "Mac"`.
- Non-functional: Swift 6 strict, 0 warnings, files under 200 lines, no engine call on main.

## Architecture
```
FsyncMac (exec) ── @main FsyncMacApp ── @StateObject EngineBridge
                                         │ boot(): AppConfig + AppSecrets → EngineConfig
                                         │ FsyncEngine(cfg, EngineDelegateAdapter)
EngineDelegateAdapter (Sendable, callback thread)
   onSessionChanged → AppSecrets.saveSession (sync)
   other callbacks  → DispatchQueue.main → bridge.receive(EngineEvent)
EngineBridge (@MainActor): phase, banner, engine; EngineCall.run { blocking } -> Result
```
`AppPhase`: `.booting`, `.signedOut`, `.needsPairing`, `.ready`, `.bootFailed(String, canReset: Bool)`.
Mapping: `AccountState.created|ready → .ready` (created additionally sets `isFirstDevice`), `.needsPairing → .needsPairing`.

Boot flow:
1. Load the secrets and build the engine.
2. If a session is stored: `setSession` → `start` → `ensureAccount` → phase.
3. Without a session: `start` → `.signedOut`.
4. A `Network` error with a stored session gives `.ready`/`.needsPairing` unknown, so the bridge stays `.booting`, shows an "offline" banner and retries `ensureAccount` with backoff (5 s, 15 s, 60 s). `Auth` clears the tokens and gives `.signedOut`.

Error mapping (`ErrorText.message(for: Error, context: ErrorContext) -> String?`):

| FsyncError | Text / action |
|---|---|
| Network | "Can't reach fsync. Check your connection." |
| Auth, ReauthRequired | "Sign in again." + bridge → `.signedOut` |
| Invalid{msg} | context text (login: "Code is wrong or expired"; pairing: "Code invalid or expired") |
| NotFound | "This changed. Review it again." |
| PeerOffline | "That device is offline." |
| HistoryKeyMismatch | boot failure, `canReset: true` |
| Cancelled | nil (silent) |
| Io, Integrity, TooLarge, NotPaired, Internal | generic "Something went wrong (code)" + `os_log` with the detail |

## Related Code Files
Create (all under `apps/macos/FsyncMac/`):
- `Package.swift`, `.gitignore` (`.build/`)
- `Sources/FsyncMacCore/Bridge/AppConfig.swift`: config resolution
- `Sources/FsyncMacCore/Bridge/KeychainStore.swift`: get/set/delete `Data` for (service, account); probes the data-protection keychain once and falls back to the file keychain on -34018; always sets `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`
- `Sources/FsyncMacCore/Bridge/AppSecrets.swift`: `loadOrCreateKey(_:)`, `session()`, `saveSession`, `clearSession`
- `Sources/FsyncMacCore/Bridge/EngineCall.swift`: `static func run<T: Sendable>(_ engine:, _ body: @Sendable (FsyncEngineProtocol) throws -> T) async -> Result<T, Error>`
- `Sources/FsyncMacCore/Bridge/EngineDelegateAdapter.swift`: `EngineEvent` enum + adapter
- `Sources/FsyncMacCore/Bridge/EngineBridge.swift`: state, boot, signOut
- `Sources/FsyncMacCore/Bridge/EngineBridge+Events.swift`: `receive(_:)` routing (extended in phases 2-4)
- `Sources/FsyncMacCore/Bridge/AppPhase.swift`, `Sources/FsyncMacCore/Bridge/ErrorText.swift`
- `Sources/FsyncMacUI/RootView.swift`: switch on phase (placeholders), boot-failure view with "Start fresh" behind a confirm
- `Sources/FsyncMac/FsyncMacApp.swift`: `MenuBarExtra(.window)` + `Window(id: "main")`
- `Sources/FsyncMac/AppDelegate.swift`: `applicationWillTerminate` → `engine.stop()`
- `Tests/FsyncMacCoreTests/{KeychainStoreTests, AppSecretsTests, AppPhaseMappingTests, ErrorTextTests, EngineBridgeBootTests}.swift`

## FFI calls
`FsyncEngine(cfg:delegate:)`, `start`, `stop`, `setSession`, `ensureAccount`, `nodeId`, free `resetHistory(dataDir:)` (only after the user confirms "Start fresh").

## Implementation Steps
1. Spike (≤1h): in a test, `SecItemAdd` with `kSecUseDataProtectionKeychain` under `swift test` and in an ad-hoc signed bundle. Record which store is used. This decides the fallback.
2. Package.swift: local dependency `../Packages/FsyncCoreKit`, three targets plus two test targets. Run `swift build`.
3. AppConfig, KeychainStore, AppSecrets. Service name injectable (default `app.fsync.mac`); accounts `device_secret`, `history_key`, `session`.
4. EngineCall, ErrorText, AppPhase (pure, with tests).
5. EngineDelegateAdapter: holds `let forward: @Sendable (EngineEvent) -> Void` and `let secrets: AppSecrets` (Sendable struct).
6. EngineBridge: `boot()`, `signOut()` (`logout` + `clearSession` → `.signedOut`), retry on Network. On `HistoryKeyMismatch` the phase becomes `.bootFailed(canReset: true)`, and "Start fresh" (confirm dialog) runs `resetHistory` and then boots again.
7. App target: MenuBarExtra icon + popover showing the phase text; Window "main" opened via `openWindow` + `NSApp.activate(ignoringOtherApps:)`.
8. Tests (below); `swift test` green.

## Todo List
- [x] Keychain spike + decision noted in KeychainStore header comment
- [x] Package.swift + targets build
- [x] AppConfig / KeychainStore / AppSecrets + tests
- [x] EngineCall / ErrorText / AppPhase + tests
- [x] Adapter + EngineBridge boot + tests
- [x] App target with MenuBarExtra + main Window

## Success Criteria
- `swift test` green with 0 warnings. Tests cover:
  - Keychain round-trip and delete, with the test service removed in teardown.
  - Keys are stable across two `AppSecrets` instances and are 32 bytes.
  - Boot with no session → `.signedOut`.
  - Boot with fake tokens against `127.0.0.1:9` → still `.booting` + offline banner, never a crash.
  - A corrupt `history.db` yields a `HistoryReset` event that the bridge receives on the main thread.
  - A wrong history key → `.bootFailed(canReset: true)`.
- `swift run FsyncMac` shows the menubar icon.

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| Data-protection keychain unavailable (ad-hoc) | H×M | Probe and fall back to the file keychain, documented. Unresolved Q2. |
| Keychain ACL prompts after each ad-hoc rebuild | M×L | Dev-only. The tests use their own service. Documented in the README section. |
| Callback reorder or main-thread deadlock | L×H | FIFO main queue. The adapter never calls the engine synchronously from main. |
| `deinit` of the engine on main blocks up to 5 s | M×M | `stop()` off main in `applicationWillTerminate`, via EngineCall |

## Security Considerations
- Secrets never go to UserDefaults, files or logs. `os_log` uses `privacy: .private` for email and codes.
- `ensureAccount`/pairing pins trust in the core. The app does not handle signatures.
- "Start fresh" is destructive (history moved aside, pin dropped), so it needs an explicit confirm.

## Next Steps
Phase 2 adds `LoginModel` and fills the `.signedOut` placeholder.
