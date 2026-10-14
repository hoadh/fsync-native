import AppKit
import Combine
import FsyncCoreKit
import Foundation
import TrustResetPrompt
import os

// FsyncCoreKit exports its own `Published` struct, so the property wrapper is qualified.

/// The app's single door to the engine: boot, session, account phase. Blocking calls go through
/// `EngineCall`; callbacks arrive via `receive(_:)` on the main actor, in order.
@MainActor
public final class EngineBridge: ObservableObject {
    @Combine.Published public internal(set) var phase: AppPhase = .booting
    /// Non-fatal status such as "offline"; nil when all is well.
    @Combine.Published public internal(set) var banner: String?
    @Combine.Published public internal(set) var isFirstDevice = false
    /// Set when the engine moved a corrupt history aside (`AccountEvent.historyReset`).
    @Combine.Published public internal(set) var historyResetBackup: String?

    public internal(set) var engine: FsyncEngineProtocol?
    public internal(set) var loginModel: LoginModel?
    public internal(set) var recoveryModel: AccountRecoveryModel?
    public internal(set) var pairingModel: PairingModel?
    public internal(set) var devicesModel: DevicesModel?
    public internal(set) var trustModel: TrustResetPromptModel?
    public internal(set) var clipSync: ClipboardSync?
    public internal(set) var clipSettings: ClipSettingsModel?
    public internal(set) var historyModel: HistoryModel?
    public internal(set) var filesSync: FilesSync?
    public internal(set) var transfers: TransfersModel?
    var fileWriter: FileClipWriter?
    var hotkey: HotkeyManager?
    var inboxCleanup: Task<Void, Never>?
    /// Set by `prepareToQuit`. Quit is final: it is never cancelled, so no clipboard start path runs
    /// afterwards and sync stays off until the next boot (models reinstalled).
    var isQuitting = false
    /// True while at least one other device is on LAN or relay (drives the menubar icon).
    @Combine.Published public internal(set) var anyPeerOnline = false
    /// The app shows the main window for each value (trust reset arrived, first launch while not ready).
    public let windowRequest = PassthroughSubject<Void, Never>()
    var modelSubscriptions: Set<AnyCancellable> = []

    let config: AppConfig
    let secrets: AppSecrets
    let dataDir: URL
    /// The pasteboard to sync; tests use a private one.
    let pasteboard: NSPasteboard
    /// Where files of received clips are downloaded.
    let inbox: InboxDirectory
    /// Where accepted "Send to…" files land.
    let downloads: URL
    /// Seconds between `ensureAccount` retries while offline; the last value repeats.
    let retryDelays: [Double]
    var started = false
    /// The pending revoke of an earlier sign-out is retried once per launch.
    var pendingRevokeTried = false
    /// Test seam: runs on the engine thread just before `setSession`/`start` during boot.
    var beforeEngineStart: (@Sendable () -> Void)?
    /// The Keychain delete started by the last `requireSignIn`; awaited by `signOut` and `shutdown`.
    var sessionClear: Task<Void, Never>?
    /// Shown as the banner once boot has started (the boot itself clears the banner).
    var startupNotice: String?
    /// Closed while signed out; see `SessionGate`.
    let sessionGate = SessionGate()
    var bootTask: Task<Void, Never>?
    nonisolated static let log = Logger(subsystem: AppConfig.bundleId, category: "bridge")

    public init(config: AppConfig, secrets: AppSecrets, dataDir: URL, pasteboard: NSPasteboard = .general, inbox: InboxDirectory = .standard(),
                downloads: URL = FilesSync.defaultDownloads(),
                retryDelays: [Double] = [5, 15, 60]) {
        self.config = config
        self.secrets = secrets
        self.dataDir = dataDir
        self.pasteboard = pasteboard
        self.inbox = inbox
        self.downloads = downloads
        self.retryDelays = retryDelays.isEmpty ? [60] : retryDelays
    }

    /// The bridge for the running app.
    public static func live() -> EngineBridge {
        let dir: URL
        var notice: String?
        do { dir = try AppConfig.defaultDataDir() } catch {
            // Fall back to a private temp dir so the app still starts. The OS may clear it, so the
            // history may not survive a restart; the banner says so.
            log.error("data dir unavailable: \(String(describing: error), privacy: .private)")
            dir = AppConfig.fallbackDataDir()
            notice = "Couldn't open the app's data folder. Your clipboard history may not be kept."
        }
        let bridge = EngineBridge(config: .live(), secrets: AppSecrets(), dataDir: dir)
        bridge.startupNotice = notice
        return bridge
    }

    /// Maps an account state to a phase. Used by boot, sign-in and recovery alike.
    public func applyAccountState(_ state: AccountState) {
        isFirstDevice = state == .created
        banner = startupNotice
        phase = AppPhase.from(state)
    }

    /// "Done" on the first-device screen.
    public func dismissFirstDevice() { isFirstDevice = false }

    /// Drops the stored session and goes to the sign-in screen.
    public func requireSignIn(banner text: String? = ErrorText.signInAgain) {
        bootTask?.cancel()
        // Off the main actor (a Keychain call can block). The gate is already closed below, so no
        // refreshed token can be stored in the meantime; deletes run in order.
        let (secrets, previous) = (secrets, sessionClear)
        sessionClear = Task.detached {
            await previous?.value
            do { try secrets.clearSession() } catch {
                Self.log.error("could not clear session: \(String(describing: error), privacy: .private)")
            }
        }
        sessionGate.set(open: false)
        isFirstDevice = false
        banner = text
        loginModel?.reset()
        resetAccountModels()
        phase = .signedOut
    }

    /// The Mac must be paired again: used when another device removed it.
    func routeToPairing() {
        pairingModel?.dismiss()
        devicesModel?.clear()
        phase = .needsPairing
    }

    /// Called after `loginVerify`: tokens are already stored. Resolves the account, retrying when offline.
    public func didSignIn() {
        bootTask?.cancel()
        sessionGate.set(open: true)
        banner = nil
        phase = .booting
        bootTask = Task { [weak self] in await self?.resolveAccount() }
    }

    /// Ends the session on the server (best effort) and locally.
    public func signOut() async {
        bootTask?.cancel()
        sessionGate.set(open: false)
        await logoutKeepingRevoke()
        requireSignIn(banner: nil)
        await sessionClear?.value
    }

    /// Stops the engine off the main thread and releases it there too. Safe to call twice.
    public func shutdown() async {
        bootTask?.cancel()
        await sessionClear?.value
        guard let engine else { return }
        removeClipboardModels()
        self.engine = nil
        loginModel = nil
        recoveryModel = nil
        pairingModel = nil
        devicesModel = nil
        trustModel = nil
        modelSubscriptions = []
        started = false
        await Task.detached {
            do { try engine.stop() } catch { Self.log.error("stop failed: \(String(describing: error), privacy: .private)") }
        }.value
    }

    /// After the user confirmed: moves the old history aside, then boots again.
    public func startFresh() async {
        guard case .bootFailed(_, canReset: true) = phase else { return }
        let dir = dataDir.path
        let result = await Task.detached { Result { try resetHistory(dataDir: dir) } }.value
        switch result {
        case .success: boot()
        case .failure(let error):
            phase = .bootFailed(ErrorText.message(for: error, context: .general) ?? "Reset failed", canReset: false)
        }
    }
}
