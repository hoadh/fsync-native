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

    func testCopyBackOnlyForTextStoredInFull() {
        func item(_ kind: ClipKind, _ preview: String?, _ size: UInt64) -> HistoryItem {
            HistoryItem(id: "aa", outgoing: false, peer: nil, kind: kind, createdMs: 1, totalSize: size, preview: preview, pinned: false)
        }
        XCTAssertEqual(item(.text, "héllo", 6).copyBackText, "héllo")
        XCTAssertNil(item(.text, String(repeating: "a", count: 200), 5000).copyBackText, "truncated preview")
        XCTAssertNil(item(.text, "", 0).copyBackText)
        XCTAssertNil(item(.text, nil, 3).copyBackText)
        XCTAssertNil(item(.image, "img", 3).copyBackText)
    }
}
