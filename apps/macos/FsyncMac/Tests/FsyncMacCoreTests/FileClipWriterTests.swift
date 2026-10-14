import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Plays the engine side of `fetch_file_to`: writes the file, then reports `Done` on the main queue like
/// the FFI event callback does. Everything else (writer, fetcher, model, pasteboard) is the real code.
final class FakeFetchBackend: @unchecked Sendable {
    nonisolated(unsafe) var transfers: TransfersModel!
    private let lock = NSLock()
    private var next: UInt64 = 0
    private(set) var started: [String] = []
    var failWith: String?
    var delay: TimeInterval = 0
    var cancelled = false

    func start(itemId: String, fileId: UInt32, destDir: String) throws -> UInt64 {
        lock.lock(); next += 1; let id = next; started.append(itemId); lock.unlock()
        let path = destDir + "/file\(fileId).txt"
        let failure = failWith
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [self] in
            MainActor.assumeIsolated {
                if cancelled {
                    transfers.handle(TransfersModelTests.event(id, .cancelled))
                } else if let failure {
                    transfers.handle(TransfersModelTests.event(id, .failed, message: failure))
                } else {
                    FileManager.default.createFile(atPath: path, contents: Data("content \(fileId)".utf8))
                    transfers.handle(TransfersModelTests.event(id, .done, done: 9, total: 9, path: path))
                }
            }
        }
        return id
    }
}

@MainActor
final class FileClipWriterTests: EngineTestCase {
    private struct Rig {
        let writer: FileClipWriter
        let backend: FakeFetchBackend
        let pb: NSPasteboard
        let inbox: InboxDirectory
    }

    private func rig(_ strategy: FileClipStrategy) -> Rig {
        let pb = scratchPasteboard()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("fsync-fcw-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let transfers = TransfersModel()
        let backend = FakeFetchBackend()
        backend.transfers = transfers
        let fetcher = FileFetchService(transfers: transfers) { try backend.start(itemId: $0, fileId: $1, destDir: $2) }
        let inbox = InboxDirectory(root: root)
        return Rig(writer: FileClipWriter(pasteboard: pb, fetcher: fetcher, inbox: inbox, strategy: { strategy }),
                   backend: backend, pb: pb, inbox: inbox)
    }

    private func clip(_ sizes: [UInt64], id: String = "0a1b2c") -> ClipItem {
        ClipItem(itemId: id, from: "node", fromName: "Other", kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: sizes.enumerated().map { FileInfo(fileId: UInt32($0.offset), name: "file\($0.offset).txt", relPath: "file\($0.offset).txt", size: $0.element) },
                 totalSize: sizes.reduce(0, +), push: false)
    }

    func testEagerWritesRealFileUrlsThatAreReadable() async throws {
        let r = rig(.eager)
        let result = await r.writer.apply(clip([9, 9]))
        XCTAssertNil(result.failure)
        let urls = try XCTUnwrap(r.pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL])
        XCTAssertEqual(urls.count, 2)
        XCTAssertEqual(try String(contentsOf: urls[1], encoding: .utf8), "content 1")
        XCTAssertTrue(urls[0].path.hasPrefix(try XCTUnwrap(try? r.inbox.folder(forItem: "0a1b2c")).path))
        XCTAssertTrue(r.pb.types?.contains(NSPasteboard.PasteboardType(SensitiveFilter.originType)) ?? false)
        XCTAssertTrue(r.writer.isCurrent)
        XCTAssertEqual(r.writer.currentItemId, "0a1b2c")
    }

    func testLazyFetchesNothingUntilReadAndThenServesTheFile() async throws {
        let r = rig(.lazy)
        let result = await r.writer.apply(clip([40 << 20]))
        XCTAssertNil(result.failure)
        XCTAssertTrue(r.backend.started.isEmpty)
        r.backend.delay = 0.2
        // A reader reaches the provider from a run loop source (AppKit's pasteboard callback), not from
        // inside a dispatch block; only then can the nested run loop deliver the transfer events.
        let pb = r.pb
        let urls = try await withCheckedThrowingContinuation { (done: CheckedContinuation<[URL], Error>) in
            nonisolated(unsafe) let finish = done
            RunLoop.main.perform(inModes: [.default]) {
                let read = pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
                finish.resume(with: read.map { .success($0) } ?? .failure(CocoaError(.fileReadUnknown)))
            }
        }
        XCTAssertEqual(r.backend.started, ["0a1b2c"])
        XCTAssertEqual(try String(contentsOf: urls[0], encoding: .utf8), "content 0")
    }

    func testHybridMixesEagerSmallAndLazyLargeFiles() async throws {
        let r = rig(.hybrid)
        _ = await r.writer.apply(clip([9, 100 << 20]))
        XCTAssertEqual(r.backend.started.count, 1)
        XCTAssertEqual(r.pb.pasteboardItems?.count, 2)
    }

    func testFailedEagerFetchLeavesPasteboardAlone() async {
        let r = rig(.eager)
        r.pb.clearContents()
        r.pb.setString("before", forType: .string)
        r.backend.failWith = "peer offline"
        let result = await r.writer.apply(clip([9]))
        XCTAssertEqual(result.failure, .fetchFailed)
        XCTAssertEqual(r.pb.string(forType: .string), "before")
        XCTAssertFalse(r.writer.isCurrent)
    }

    func testClipWithoutFilesAndHostileItemIdAreRejected() async {
        let r = rig(.eager)
        let none = await r.writer.apply(clip([]))
        XCTAssertEqual(none.failure, .nothingUsable)
        let hostile = await r.writer.apply(clip([9], id: "../../etc"))
        XCTAssertEqual(hostile.failure, .fetchFailed)
        XCTAssertTrue(r.backend.started.isEmpty)
    }

    func testClearIfOursRemovesOnlyAnUntouchedLazyEntry() async {
        let r = rig(.lazy)
        _ = await r.writer.apply(clip([40 << 20]))
        r.writer.clearIfOurs()
        XCTAssertTrue(r.pb.types?.isEmpty ?? true)
        _ = await r.writer.apply(clip([40 << 20]))
        r.pb.clearContents()
        r.pb.setString("user copy", forType: .string)
        r.writer.clearIfOurs()
        XCTAssertEqual(r.pb.string(forType: .string), "user copy")
    }
}
