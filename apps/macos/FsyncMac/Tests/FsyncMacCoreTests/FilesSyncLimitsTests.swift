import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class FilesSyncLimitsTests: EngineTestCase {
    private func make(autoAccept: Bool) throws -> (FilesSync, FakeFetchBackend) {
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        let fetcher = FileFetchService(transfers: transfers) { try backend.start(itemId: $0, fileId: $1, destDir: $2) }
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        let sync = FilesSync(engine: engine, fetcher: fetcher, downloads: try tempDir()) { autoAccept }
        sync.start()
        return (sync, backend)
    }

    private func offer(_ id: String, files: Int = 1, total: UInt64 = 9) -> ClipItem {
        ClipItem(itemId: id, from: "node", fromName: "Phone", kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: (0..<files).map { FileInfo(fileId: UInt32($0), name: "f\($0).txt", relPath: "f\($0).txt", size: 9) },
                 totalSize: total, push: true)
    }

    func testPendingOffersAreCappedOldestDropped() async throws {
        let (sync, _) = try make(autoAccept: false)
        for i in 0..<(FilesSync.maxOffers + 5) { await sync.receive(offer: offer(String(format: "%04x", i))) }
        XCTAssertEqual(sync.offers.count, FilesSync.maxOffers)
        XCTAssertEqual(sync.offers.first?.itemId, String(format: "%04x", FilesSync.maxOffers + 4))
        XCTAssertFalse(sync.offers.contains { $0.itemId == "0000" })
    }

    func testAutoAcceptWaitsForTheUserAboveTheLimit() async throws {
        let (sync, backend) = try make(autoAccept: true)
        await sync.receive(offer: offer("0a", total: FilesSync.autoAcceptLimit + 1))
        XCTAssertEqual(sync.offers.map(\.itemId), ["0a"])
        XCTAssertTrue(backend.started.isEmpty)
        await sync.receive(offer: offer("0b", total: FilesSync.autoAcceptLimit))
        XCTAssertEqual(backend.started, ["0b"])
    }

    func testCancelAbortsTheRestOfTheOffer() async throws {
        let (sync, backend) = try make(autoAccept: false)
        backend.cancelled = true
        await sync.receive(offer: offer("0c", files: 3))
        await sync.accept(sync.offers[0])
        XCTAssertEqual(backend.started.count, 1, "no further files after a cancel")
        XCTAssertNil(sync.message, "a cancel is not a failure")
    }
}
