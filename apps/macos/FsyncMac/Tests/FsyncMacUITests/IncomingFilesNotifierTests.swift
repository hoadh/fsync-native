import Combine
import UserNotifications
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

@MainActor
private final class ManualScheduler {
    var pending: [@MainActor () -> Void] = []
    func step() -> Bool {
        guard !pending.isEmpty else { return false }
        pending.removeFirst()()
        return true
    }
}

@MainActor
private final class FakePoster: NotificationPosting {
    var installed = 0
    var authorizations = 0
    var allow = true
    var posted: [IncomingFilesNotification] = []
    var removed: [String] = []

    func install(delegate: UNUserNotificationCenterDelegate) { installed += 1 }
    func requestAuthorization() async -> Bool {
        authorizations += 1
        return allow
    }
    func post(_ notification: IncomingFilesNotification) async throws { posted.append(notification) }
    func removeDelivered(requestID: String) { removed.append(requestID) }
}

@MainActor
final class IncomingFilesNotifierTests: XCTestCase {
    private let appURL = URL(fileURLWithPath: "/Applications/fsync.app")
    private var windowRequests = 0
    private var bag = Set<AnyCancellable>()
    private var transfersVisible = false
    private var mainWindowVisible = false
    private let scheduler = ManualScheduler()

    private func makeBridge() -> EngineBridge {
        let bridge = EngineBridge(
            config: AppConfig(apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "t"),
            secrets: AppSecrets(), dataDir: FileManager.default.temporaryDirectory)
        windowRequests = 0
        bridge.windowRequest.sink { [unowned self] in windowRequests += 1 }.store(in: &bag)
        return bridge
    }

    private func make(
        _ bridge: EngineBridge, _ poster: FakePoster, bundle: URL? = nil
    ) -> IncomingFilesNotifier {
        let notifier = IncomingFilesNotifier(
            bridge: bridge, bundleURL: bundle ?? appURL, makePoster: { poster },
            isTransfersVisible: { [unowned self] in transfersVisible },
            isMainWindowVisible: { [unowned self] in mainWindowVisible },
            schedule: { [scheduler] _, block in scheduler.pending.append(block) })
        notifier.start()
        return notifier
    }

    private let notice = IncomingFilesNotice(fromName: "Alice", fileCount: 2, totalBytes: 2048, autoAccepted: true)

    func testNoBundleIsNoOp() async {
        let bridge = makeBridge(), poster = FakePoster()
        var factoryCalls = 0
        let notifier = IncomingFilesNotifier(
            bridge: bridge, bundleURL: URL(fileURLWithPath: "/tmp/.build/debug/FsyncMac"),
            makePoster: { factoryCalls += 1; return poster })
        notifier.start()
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(factoryCalls, 0, "the real center must never be created outside an app bundle")
        XCTAssertEqual(poster.installed, 0)
        XCTAssertEqual(poster.authorizations, 0)
        XCTAssertTrue(poster.posted.isEmpty)
    }

    func testNoticePostsExactSpecTextWithoutFileData() async {
        let bridge = makeBridge(), poster = FakePoster()
        let notifier = make(bridge, poster)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(poster.installed, 1)
        XCTAssertEqual(poster.posted.count, 1)
        XCTAssertEqual(poster.posted.first?.title, "Files from Alice")
        XCTAssertEqual(poster.posted.first?.body, notice.body)
        XCTAssertTrue(poster.posted.first?.body.hasPrefix("2 files, ") ?? false)
        XCTAssertTrue(poster.posted.first?.body.hasSuffix("Saving to Downloads") ?? false)
    }

    func testAuthorizationIsLazyAndDenialDropsSilently() async {
        let bridge = makeBridge(), poster = FakePoster()
        poster.allow = false
        let notifier = make(bridge, poster)
        XCTAssertEqual(poster.authorizations, 0, "lazy: nothing asked before the first notice")
        bridge.incomingFiles.send(notice)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(poster.authorizations, 2, "denied: each notice re-checks")
        XCTAssertTrue(poster.posted.isEmpty)
    }

    func testSuppressedWhenTransfersVisible() async {
        let bridge = makeBridge(), poster = FakePoster()
        transfersVisible = true
        let notifier = make(bridge, poster)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertTrue(poster.posted.isEmpty)
        XCTAssertEqual(poster.authorizations, 0)
    }

    func testArrivalNeverRaisesAWindowOrNavigates() async {
        let bridge = makeBridge(), poster = FakePoster()
        bridge.navigation.show(.history)
        let notifier = make(bridge, poster)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(poster.posted.count, 1)
        XCTAssertEqual(windowRequests, 0)
        XCTAssertEqual(bridge.navigation.section, .history)
    }

    func testClickRoutesToTransfersWithOneWindowRequest() {
        let bridge = makeBridge(), poster = FakePoster()
        bridge.navigation.show(.history)
        let notifier = make(bridge, poster)
        notifier.routeClick()
        XCTAssertEqual(bridge.navigation.section, .transfers)
        XCTAssertEqual(windowRequests, 1)
    }

    func testDenialIsNotCachedAndGrantIsPickedUp() async {
        let bridge = makeBridge(), poster = FakePoster()
        poster.allow = false
        let notifier = make(bridge, poster)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertTrue(poster.posted.isEmpty)
        poster.allow = true  // enabled in System Settings meanwhile
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(poster.authorizations, 2)
        XCTAssertEqual(poster.posted.count, 1)
        bridge.incomingFiles.send(notice)
        await notifier.drain()
        XCTAssertEqual(poster.authorizations, 2, "a grant is remembered")
    }

    func testPostsKeepArrivalOrder() async {
        let bridge = makeBridge(), poster = FakePoster()
        let notifier = make(bridge, poster)
        let first = IncomingFilesNotice(fromName: "Alice", fileCount: 1, totalBytes: 1, autoAccepted: true)
        let update = IncomingFilesNotice(fromName: "Alice", fileCount: 3, totalBytes: 3, autoAccepted: true, offerCount: 2)
        bridge.incomingFiles.send(first)
        bridge.incomingFiles.send(update)
        await notifier.drain()
        XCTAssertEqual(poster.posted.map(\.body), [first.body, update.body])
    }

    /// The window never appears: the retry loop sends once now, then once per tick, and stops at the cap.
    func testColdLaunchRetriesAreBoundedWhenWindowNeverAppears() {
        let notifier = make(makeBridge(), FakePoster())
        notifier.routeClick(requestID: "fsync.incoming.Alice")
        XCTAssertEqual(windowRequests, 1)
        while scheduler.step() {}
        XCTAssertEqual(windowRequests, 1 + IncomingFilesNotifier.maxRetries)
        XCTAssertTrue(scheduler.pending.isEmpty)
    }

    func testColdLaunchRetriesStopOnceWindowIsVisible() {
        let notifier = make(makeBridge(), FakePoster())
        notifier.routeClick()
        for _ in 0..<3 { XCTAssertTrue(scheduler.step()) }
        mainWindowVisible = true
        XCTAssertTrue(scheduler.step())
        XCTAssertFalse(scheduler.step(), "stopped, nothing left queued")
        XCTAssertEqual(windowRequests, 1 + 3)
    }

    func testVisibleWindowGetsOnlyTheInitialRequest() {
        mainWindowVisible = true
        let notifier = make(makeBridge(), FakePoster())
        notifier.routeClick()
        while scheduler.step() {}
        XCTAssertEqual(windowRequests, 1)
    }

    func testSuppressionIsRecheckedAtPostTime() async {
        let bridge = makeBridge(), poster = FakePoster()
        let notifier = make(bridge, poster)
        bridge.incomingFiles.send(notice)
        transfersVisible = true  // the user opened Transfers before the queued post ran
        await notifier.drain()
        XCTAssertTrue(poster.posted.isEmpty)
    }
}
