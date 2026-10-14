import XCTest
@testable import FsyncMacCore

final class FileClipStrategyAndInboxTests: XCTestCase {
    private let mb: UInt64 = 1 << 20

    func testHybridFetchesSmallFilesNowAndLargeOnesLater() {
        XCTAssertEqual(FileClipStrategy.hybrid.eagerFlags(sizes: [mb, 19 * mb, 20 * mb, 300 * mb]), [true, true, false, false])
    }

    func testLazyNeverAndEagerAlways() {
        XCTAssertEqual(FileClipStrategy.lazy.eagerFlags(sizes: [1, 2]), [false, false])
        XCTAssertEqual(FileClipStrategy.eager.eagerFlags(sizes: [1, 300 * mb]), [true, true])
    }

    func testEagerBytesOfOneClipAreCapped() {
        XCTAssertEqual(FileClipStrategy.eager.eagerFlags(sizes: [300 * mb, 300 * mb, mb]), [true, false, true])
    }

    func testStoredStrategyDefaultsToHybridAndRoundTrips() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "app.fsync.mac.tests.\(UUID().uuidString)"))
        XCTAssertEqual(FileClipStrategy.stored(defaults), .hybrid)
        FileClipStrategy.eager.store(defaults)
        XCTAssertEqual(FileClipStrategy.stored(defaults), .eager)
        defaults.set("bogus", forKey: FileClipStrategy.defaultsKey)
        XCTAssertEqual(FileClipStrategy.stored(defaults), .hybrid)
    }

    private func inbox() -> InboxDirectory {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("fsync-inbox-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return InboxDirectory(root: root)
    }

    func testFolderIsUserOnlyAndInsideRoot() throws {
        let inbox = inbox()
        let dir = try inbox.folder(forItem: "0a1b2c3d-4e5f")
        XCTAssertEqual(dir.deletingLastPathComponent().standardizedFileURL, inbox.root.standardizedFileURL)
        let perms = try FileManager.default.attributesOfItem(atPath: dir.path)[.posixPermissions] as? Int
        XCTAssertEqual(perms, 0o700)
    }

    func testHostileItemIdsAreRejected() {
        let inbox = inbox()
        for bad in ["", "..", "../x", "a/b", "zz", String(repeating: "a", count: 65), "é1"] {
            XCTAssertThrowsError(try inbox.folder(forItem: bad), bad)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: inbox.root.path))
    }

    func testRemoveOldKeepsRecentAndProtectedFolders() throws {
        let inbox = inbox()
        let (old, keep, fresh) = (try inbox.folder(forItem: "aa"), try inbox.folder(forItem: "bb"), try inbox.folder(forItem: "cc"))
        let past = Date().addingTimeInterval(-10 * 86_400)
        for url in [old, keep] { try FileManager.default.setAttributes([.modificationDate: past], ofItemAtPath: url.path) }
        XCTAssertEqual(inbox.removeOld(before: Date().addingTimeInterval(-86_400), keeping: ["bb"]), 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: old.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: keep.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.path))
    }

    func testRemoveOldOnMissingRootIsZero() {
        XCTAssertEqual(inbox().removeOld(before: Date(), keeping: []), 0)
    }
}
