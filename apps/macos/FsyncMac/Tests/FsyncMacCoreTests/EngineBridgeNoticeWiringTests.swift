import Combine
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Offer -> `FilesSync` -> `bridge.incomingFiles`, with the real bridge and an offline engine.
@MainActor
final class EngineBridgeNoticeWiringTests: EngineTestCase {
    private var cancellables: Set<AnyCancellable> = []

    private func readyBridge() async throws -> EngineBridge {
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        bridge.applyAccountState(.ready)
        await eventually("files running") { bridge.filesSync != nil }
        return bridge
    }

    private func offer(_ id: String, from name: String = "Phone\u{202E}") -> ClipItem {
        ClipItem(itemId: id, from: "node", fromName: name, kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: [FileInfo(fileId: 0, name: "secret-name.txt", relPath: "x", size: 5)], totalSize: 5, push: true)
    }

    func testOfferEmitsOneSanitizedNoticeAndDuplicateDoesNot() async throws {
        let bridge = try await readyBridge()
        var seen: [IncomingFilesNotice] = []
        bridge.incomingFiles.sink { seen.append($0) }.store(in: &cancellables)
        let sync = try XCTUnwrap(bridge.filesSync)
        await sync.receive(offer: offer("aa"))
        await sync.receive(offer: offer("aa"))
        XCTAssertEqual(seen.count, 1)
        XCTAssertEqual(seen[0].title, "Files from Phone")
        XCTAssertFalse(seen[0].autoAccepted)
        XCTAssertFalse(seen[0].body.contains("secret-name"))
        await bridge.shutdown()
    }

    func testSecondOfferInTheWindowIsHeldBackThenFlushedAsOne() async throws {
        let bridge = try await readyBridge()
        // Short window so the test does not wait 10 s.
        let sync = FilesSync(engine: try XCTUnwrap(bridge.engine), fetcher: FileFetchService(transfers: TransfersModel()) { _, _, _ in 1 },
                             downloads: try tempDir(), noticeWindow: 0.3) { false }
        sync.start()
        var seen: [IncomingFilesNotice] = []
        sync.onNotice = { seen.append($0) }
        await sync.receive(offer: offer("aa"))
        await sync.receive(offer: offer("bb"))
        XCTAssertEqual(seen.count, 1)
        await eventually("flushed") { seen.count == 2 }
        XCTAssertEqual(seen[1].offerCount, 1)
        XCTAssertTrue(seen[1].body.hasPrefix("1 file, "), "one held offer reads as itself")
        sync.stop()
        await bridge.shutdown()
    }

    func testAutoAcceptAnnouncesBeforeTheDownloadStarts() async throws {
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        let fetcher = FileFetchService(transfers: transfers) { try backend.start(itemId: $0, fileId: $1, destDir: $2) }
        let sync = FilesSync(engine: try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore())), fetcher: fetcher,
                             downloads: try tempDir()) { true }
        sync.start()
        var startedWhenAnnounced: [Int] = []
        var notices: [IncomingFilesNotice] = []
        sync.onNotice = { notices.append($0); startedWhenAnnounced.append(backend.started.count) }
        await sync.receive(offer: offer("aa"))
        XCTAssertEqual(startedWhenAnnounced, [0])
        XCTAssertEqual(backend.started.count, 1)
        XCTAssertTrue(notices[0].autoAccepted)
        XCTAssertTrue(notices[0].body.hasSuffix("Saving to Downloads"))
    }

    func testRetryTrustFollowsTheDeviceList() async throws {
        let bridge = try await readyBridge()
        let sync = try XCTUnwrap(bridge.filesSync)
        XCTAssertFalse(sync.isPeerTrusted("node"))
        let device = Device(nodeId: "node", name: "Phone", platform: .mac, status: .lan, online: true, lastSeenMs: 1, isSelf: false)
        try XCTUnwrap(bridge.devicesModel).merge(device)
        XCTAssertTrue(sync.isPeerTrusted("node"))
        try XCTUnwrap(bridge.devicesModel).clear()
        XCTAssertFalse(sync.isPeerTrusted("node"))
        await bridge.shutdown()
    }
}
