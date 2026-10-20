import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class TransfersRetryTests: EngineTestCase {
    private struct Rig {
        let sync: FilesSync
        let transfers: TransfersModel
        let backend: FakeFetchBackend
    }

    private func make() throws -> Rig {
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        let fetcher = FileFetchService(transfers: transfers) { try backend.start(itemId: $0, fileId: $1, destDir: $2) }
        let sync = FilesSync(engine: try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore())), fetcher: fetcher,
                             downloads: try tempDir()) { false }
        sync.isPeerTrusted = { $0 == "node" }
        sync.start()
        return Rig(sync: sync, transfers: transfers, backend: backend)
    }

    private func offer(from: String = "node") -> ClipItem {
        ClipItem(itemId: "0a1b", from: from, fromName: "Phone", kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: [FileInfo(fileId: 0, name: "a.txt", relPath: "a.txt", size: 9)], totalSize: 9, push: true)
    }

    /// A receive that failed, as the UI sees it.
    private func failedRow(_ rig: Rig, from: String = "node") async -> TransferRow {
        rig.backend.failWith = "peer went away"
        await rig.sync.receive(offer: offer(from: from))
        await rig.sync.accept(rig.sync.offers[0])
        rig.backend.failWith = nil
        return rig.transfers.rows[0]
    }

    func testFailedReceiveCanRetryAndRetryReplacesTheRow() async throws {
        let rig = try make()
        let row = await failedRow(rig)
        XCTAssertTrue(rig.sync.canRetry(row))
        await rig.sync.retry(row)
        XCTAssertEqual(rig.backend.started.count, 2)
        XCTAssertEqual(rig.transfers.rows.count, 1)
        XCTAssertEqual(rig.transfers.rows[0].state, .done)
        XCTAssertNotEqual(rig.transfers.rows[0].id, row.id)
        XCTAssertNotNil(rig.transfers.rows[0].revealURL)
        XCTAssertFalse(rig.sync.canRetry(row), "the old row is gone")
    }

    func testFailedStartKeepsTheRowAndSpecForAnotherTry() async throws {
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        nonisolated(unsafe) var failStart = false
        let fetcher = FileFetchService(transfers: transfers) { (item: String, file: UInt32, dir: String) -> UInt64 in
            if failStart { throw FsyncError.Network(msg: "down") }
            return try backend.start(itemId: item, fileId: file, destDir: dir)
        }
        let sync = FilesSync(engine: try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore())), fetcher: fetcher,
                             downloads: try tempDir()) { false }
        sync.isPeerTrusted = { _ in true }
        sync.start()
        backend.failWith = "x"
        await sync.receive(offer: offer())
        await sync.accept(sync.offers[0])
        backend.failWith = nil
        let row = transfers.rows[0]
        failStart = true
        await sync.retry(row)
        XCTAssertEqual(transfers.rows.map(\.id), [row.id])
        XCTAssertTrue(sync.canRetry(row))
        failStart = false
        await sync.retry(row)
        XCTAssertEqual(transfers.rows.count, 1)
        XCTAssertEqual(transfers.rows[0].state, .done)
    }

    func testDoubleTapStartsOneFetch() async throws {
        let rig = try make()
        let row = await failedRow(rig)
        rig.backend.delay = 0.2
        async let first: Void = rig.sync.retry(row)
        async let second: Void = rig.sync.retry(row)
        _ = await (first, second)
        XCTAssertEqual(rig.backend.started.count, 2, "one original + one retry")
    }

    func testNoRetryForSuccessUnknownTransferOrUntrustedPeer() async throws {
        let rig = try make()
        let row = await failedRow(rig)
        var done = row
        done.state = .done
        XCTAssertFalse(rig.sync.canRetry(done))
        var unknown = row
        unknown = TransferRow(id: 999, itemId: row.itemId, direction: .receive, state: .failed, bytesDone: 0, bytesTotal: 0)
        XCTAssertFalse(rig.sync.canRetry(unknown))
        var send = row
        send.direction = .send
        XCTAssertFalse(rig.sync.canRetry(send))
        rig.sync.isPeerTrusted = { _ in false }
        XCTAssertFalse(rig.sync.canRetry(row), "peer removed from the trusted list")
        await rig.sync.retry(row)
        XCTAssertEqual(rig.backend.started.count, 1)
    }

    func testSpecsAreClearedByPruneTrustResetStopAndSignOut() async throws {
        let rig = try make()
        var row = await failedRow(rig)
        rig.sync.pruneRetrySpecs(keepingPeers: ["node"])
        XCTAssertTrue(rig.sync.canRetry(row))
        rig.sync.pruneRetrySpecs(keepingPeers: ["other"])
        XCTAssertFalse(rig.sync.canRetry(row))

        row = await failedRow(rig)
        XCTAssertTrue(rig.sync.canRetry(row))
        rig.sync.clearRetrySpecs()
        XCTAssertFalse(rig.sync.canRetry(row))

        row = await failedRow(rig)
        rig.sync.stop()  // sign-out and every phase change away from .ready end up here
        XCTAssertFalse(rig.sync.canRetry(row))
    }

    func testRevealURLNeedsADoneRowWhoseFileExists() throws {
        let dir = try tempDir()
        let file = dir.appendingPathComponent("x.txt")
        try Data("x".utf8).write(to: file)
        let model = TransfersModel()
        model.handle(TransfersModelTests.event(1, .done, path: file.path))
        model.handle(TransfersModelTests.event(2, .done, path: dir.appendingPathComponent("missing").path))
        model.handle(TransfersModelTests.event(3, .failed, path: file.path))
        model.handle(TransfersModelTests.event(4, .started))
        XCTAssertNotNil(model.rows.first { $0.id == 1 }?.revealURL)
        XCTAssertNil(model.rows.first { $0.id == 2 }?.revealURL)
        XCTAssertNil(model.rows.first { $0.id == 3 }?.revealURL)
        XCTAssertEqual(model.finished.count, 3)
        model.remove(1)
        XCTAssertEqual(model.outcome(of: 1), .done(path: file.path), "waiters still get an answer")
        XCTAssertEqual(model.rows.count, 3)
    }
}
