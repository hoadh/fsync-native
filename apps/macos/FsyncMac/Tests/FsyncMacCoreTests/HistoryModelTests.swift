import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class HistoryModelTests: EngineTestCase {
    private func setup() throws -> (HistoryModel, FsyncEngine) {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        return (HistoryModel(engine: engine), engine)
    }

    func testNewEngineHasEmptyHistory() async throws {
        let (m, _) = try setup()
        await m.reload(query: "anything")
        XCTAssertTrue(m.items.isEmpty)
        XCTAssertFalse(m.hasMore)
        XCTAssertNil(m.errorMessage)
        XCTAssertEqual(m.query, "anything")
    }

    func testResetEmptiesTheList() async throws {
        let (m, _) = try setup()
        m.reset()
        XCTAssertTrue(m.items.isEmpty)
        XCTAssertFalse(m.hasMore)
    }

    func testResetClearsLoadingFlag() async throws {
        let (m, _) = try setup()
        m.reset()
        XCTAssertFalse(m.isLoading)
    }

    func testCopyBackIsOfferedForEverythingButFiles() {
        func item(_ kind: ClipKind) -> HistoryItem {
            HistoryItem(id: "aa", outgoing: false, peer: nil, kind: kind, createdMs: 1, totalSize: 5000, preview: "p", pinned: false)
        }
        XCTAssertTrue(item(.text).canCopyBack)
        XCTAssertTrue(item(.image).canCopyBack)
        XCTAssertFalse(item(.files).canCopyBack)
    }
}
