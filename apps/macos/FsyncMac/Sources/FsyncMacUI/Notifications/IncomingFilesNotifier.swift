import AppKit
import Combine
import FsyncMacCore
import UserNotifications
import os

/// Shows `bridge.incomingFiles` notices as user notifications. Arrival is quiet: it only posts, and never
/// activates the app, opens a window or sends `windowRequest`. Only an explicit click does that (`routeClick`).
/// Without an `.app` bundle (`swift run`) `start()` does nothing, because the real center would crash.
@MainActor
public final class IncomingFilesNotifier: NSObject, UNUserNotificationCenterDelegate {
    private static let log = Logger(subsystem: "app.fsync.mac", category: "notifications")

    private let bridge: EngineBridge
    private let bundleURL: URL
    private let makePoster: () -> NotificationPosting
    /// True when the Transfers page is on screen in the active app (then a banner adds nothing).
    private let isTransfersVisible: @MainActor () -> Bool
    private var poster: NotificationPosting?
    private var subscription: AnyCancellable?
    /// The in-flight authorization check (single-flight); cleared once it ends denied, so the next notice re-checks.
    private var authorization: Task<Bool, Never>?
    /// Posts are chained so a flushed update can never overtake the first notice.
    private var tail: Task<Void, Never>?
    private let isMainWindowVisible: @MainActor () -> Bool
    /// Runs a block after a delay on the main actor; tests replace it so no real time passes.
    private let schedule: @MainActor (TimeInterval, @escaping @MainActor () -> Void) -> Void
    /// Bumped per click, so an older click's retry loop stops when a newer click starts one.
    private var clickGeneration = 0

    /// Cold-launch retry: every `retryInterval` seconds, at most `maxRetries` times (about 3 s).
    static let retryInterval: TimeInterval = 0.25
    static let maxRetries = 12

    /// `makePoster` runs only after the bundle check, so the real center is never touched outside an app bundle.
    public init(
        bridge: EngineBridge, bundleURL: URL = Bundle.main.bundleURL,
        makePoster: @escaping () -> NotificationPosting = { UserNotificationCenterPoster() },
        isTransfersVisible: (@MainActor () -> Bool)? = nil,
        isMainWindowVisible: (@MainActor () -> Bool)? = nil,
        schedule: (@MainActor (TimeInterval, @escaping @MainActor () -> Void) -> Void)? = nil
    ) {
        self.bridge = bridge
        self.bundleURL = bundleURL
        self.makePoster = makePoster
        self.isTransfersVisible = isTransfersVisible ?? { [weak bridge] in
            NSApp.isActive && NSApp.keyWindow?.identifier == WindowIDs.main && bridge?.navigation.section == .transfers
        }
        self.isMainWindowVisible = isMainWindowVisible ?? { WindowIDs.window(WindowIDs.main)?.isVisible == true }
        self.schedule = schedule ?? { delay, block in
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { MainActor.assumeIsolated(block) }
        }
        super.init()
    }

    /// Call from `applicationDidFinishLaunching`: the delegate must exist before any click can arrive.
    public func start() {
        guard bundleURL.pathExtension == "app", poster == nil else { return }
        let poster = makePoster()
        self.poster = poster
        poster.install(delegate: self)
        subscription = bridge.incomingFiles.sink { [weak self] notice in
            MainActor.assumeIsolated { self?.handle(notice) }
        }
    }

    private func handle(_ notice: IncomingFilesNotice) {
        guard !isTransfersVisible(), let poster else { return }
        let notification = IncomingFilesNotification(notice: notice)
        let previous = tail
        tail = Task { @MainActor [weak self] in
            await previous?.value
            guard let self, await self.authorized(poster) else { return }  // denied: tray and popover still show offers
            // The user may have opened Transfers while this waited for the previous post or the permission prompt.
            guard !self.isTransfersVisible() else { return }
            do { try await poster.post(notification) } catch {
                Self.log.error("posting a notification failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    /// Waits for queued posts (tests).
    func drain() async { await tail?.value }

    /// Single-flight. A grant is remembered; a denial is not, so enabling alerts later works without a relaunch.
    private func authorized(_ poster: NotificationPosting) async -> Bool {
        if let running = authorization { return await running.value }
        let task = Task { @MainActor in await poster.requestAuthorization() }
        authorization = task
        let granted = await task.value
        if !granted { authorization = nil }
        return granted
    }

    /// An explicit click: Transfers page, window forward. The only place that raises a window.
    func routeClick(requestID: String? = nil) {
        bridge.navigation.show(.transfers)
        if let requestID { poster?.removeDelivered(requestID: requestID) }
        bridge.windowRequest.send()
        // Cold launch: the click can arrive before the app scene subscribes to `windowRequest` (no replay), and
        // the label may subscribe later still. Re-send until the main window is visible, bounded. `showMain`
        // is idempotent (it opens the single `Window` scene by id, which focuses an existing one).
        clickGeneration += 1
        retryWindowRequest(generation: clickGeneration, attempt: 1)
    }

    private func retryWindowRequest(generation: Int, attempt: Int) {
        schedule(Self.retryInterval) { [weak self] in
            guard let self, generation == self.clickGeneration, !self.isMainWindowVisible() else { return }
            self.bridge.windowRequest.send()
            if attempt < Self.maxRetries { self.retryWindowRequest(generation: generation, attempt: attempt + 1) }
        }
    }

    // MARK: UNUserNotificationCenterDelegate

    /// A banner for notifications that arrive while the app runs, unless the user is already looking at Transfers.
    public nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        await MainActor.run { isTransfersVisible() ? [] : [.banner] }
    }

    public nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        let info = response.notification.request.content.userInfo
        guard IncomingFilesNotification.isOpenFilesClick(actionIdentifier: response.actionIdentifier, userInfo: info)
        else { return }
        let id = response.notification.request.identifier
        await MainActor.run { routeClick(requestID: id) }
    }
}
