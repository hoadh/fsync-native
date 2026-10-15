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

    /// One received clip raises several events; they must share one engine read, not queue one each.
    func testRefreshCallsInOneTurnShareOneRead() async throws {
        let (m, _) = try setup()
        let before = m.generation
        let calls = (0..<3).map { _ in Task { await m.refresh() } }
        for call in calls { await call.value }
        XCTAssertEqual(m.generation, before + 1)
        XCTAssertNil(m.errorMessage)
        await m.refresh()
        XCTAssertEqual(m.generation, before + 2, "a later refresh reads again")
    }

    func testRefreshQueuedBeforeResetDoesNotReadAgain() async throws {
        let (m, _) = try setup()
        let refresh = Task { await m.refresh() }
        await Task.yield()  // the refresh is now parked at its own yield
        m.reset()
        let afterReset = m.generation
        await refresh.value
        XCTAssertEqual(m.generation, afterReset)
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
