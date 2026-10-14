import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// FilesSync with the real (offline) engine; `fetch_file_to` is the closure seam, as in the writer tests.
@MainActor
final class FilesSyncTests: EngineTestCase {
    private func make(autoAccept: Bool = false) throws -> (FilesSync, FakeFetchBackend, URL) {
        let downloads = try tempDir()
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        let fetcher = FileFetchService(transfers: transfers) { try backend.start(itemId: $0, fileId: $1, destDir: $2) }
        let sync = FilesSync(engine: try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore())), fetcher: fetcher, downloads: downloads) { autoAccept }
        sync.start()
        return (sync, backend, downloads)
    }

    private func offer(_ id: String = "0a1b", names: [String] = ["a.txt", "b.txt"]) -> ClipItem {
        ClipItem(itemId: id, from: "node", fromName: "Phone", kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: names.enumerated().map { FileInfo(fileId: UInt32($0.offset), name: $0.element, relPath: $0.element, size: 9) },
                 totalSize: 18, push: true)
    }

    func testOfferWaitsForTheUserAndAcceptDownloadsEveryFile() async throws {
        let (sync, backend, downloads) = try make()
        await sync.receive(offer: offer())
        XCTAssertEqual(sync.offers.map(\.itemId), ["0a1b"])
        XCTAssertTrue(backend.started.isEmpty)
        await sync.accept(sync.offers[0])
        XCTAssertTrue(sync.offers.isEmpty)
        XCTAssertEqual(backend.started.count, 2)
        XCTAssertNil(sync.message)
        XCTAssertTrue(FileManager.default.fileExists(atPath: downloads.appendingPathComponent("file1.txt").path))
    }

    func testAutoAcceptDownloadsWithoutAsking() async throws {
        let (sync, backend, _) = try make(autoAccept: true)
        await sync.receive(offer: offer())
        XCTAssertTrue(sync.offers.isEmpty)
        XCTAssertEqual(backend.started.count, 2)
    }

    func testDeclineDropsTheOfferWithoutDownloading() async throws {
        let (sync, backend, _) = try make()
        await sync.receive(offer: offer())
        sync.decline(sync.offers[0])
        XCTAssertTrue(sync.offers.isEmpty)
        XCTAssertTrue(backend.started.isEmpty)
    }

    func testFailedDownloadIsReportedByName() async throws {
        let (sync, backend, _) = try make()
        backend.failWith = "peer went away"
        await sync.receive(offer: offer(names: ["a.txt"]))
        await sync.accept(sync.offers[0])
        XCTAssertEqual(sync.message, "1 of 1 files couldn't be received from Phone.")
        sync.dismissMessage()
        XCTAssertNil(sync.message)
    }

    func testSameOfferTwiceIsListedOnceAndStoppedSyncIgnoresOffers() async throws {
        let (sync, _, _) = try make()
        await sync.receive(offer: offer())
        await sync.receive(offer: offer())
        XCTAssertEqual(sync.offers.count, 1)
        sync.stop()
        XCTAssertTrue(sync.offers.isEmpty)
        await sync.receive(offer: offer("ff"))
        XCTAssertTrue(sync.offers.isEmpty)
        await sync.receive(offer: offer(names: []))
        XCTAssertTrue(sync.offers.isEmpty)
    }

    func testSendOfAnUnreachableOrInvalidTargetReportsAnError() async throws {
        let (sync, _, _) = try make()
        let file = try tempDir().appendingPathComponent("x.txt")
        try Data("x".utf8).write(to: file)
        await sync.send([file], to: "00" + String(repeating: "ab", count: 31))
        XCTAssertNotNil(sync.message)
        sync.dismissMessage()
        await sync.send([URL(string: "https://example.com/x")!], to: "ab")
        XCTAssertNil(sync.message, "non-file URLs are ignored")
    }
}
