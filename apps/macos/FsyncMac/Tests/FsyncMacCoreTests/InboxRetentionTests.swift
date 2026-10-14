import XCTest
@testable import FsyncMacCore

final class InboxRetentionTests: XCTestCase {
    private func inbox() -> InboxDirectory {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("fsync-retention-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return InboxDirectory(root: root)
    }

    @discardableResult
    private func folder(_ inbox: InboxDirectory, _ id: String, bytes: Int, age: TimeInterval, now: Date) throws -> URL {
        let dir = try inbox.folder(forItem: id)
        try Data(count: bytes).write(to: dir.appendingPathComponent("f.bin"))
        try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-age)], ofItemAtPath: dir.path)
        return dir
    }

    private func names(_ inbox: InboxDirectory) -> Set<String> {
        Set(((try? FileManager.default.contentsOfDirectory(atPath: inbox.root.path)) ?? []))
    }

    func testTrimRemovesOldestFirstUntilUnderTheCap() throws {
        let inbox = inbox(), now = Date()
        try folder(inbox, "01", bytes: 64_000, age: 300, now: now)
        try folder(inbox, "02", bytes: 64_000, age: 200, now: now)
        try folder(inbox, "03", bytes: 64_000, age: 100, now: now)
        XCTAssertEqual(inbox.trim(toBytes: 140_000, keeping: []), 1)
        XCTAssertEqual(names(inbox), ["02", "03"])
    }

    func testTrimNeverRemovesKeptFolders() throws {
        let inbox = inbox(), now = Date()
        try folder(inbox, "01", bytes: 64_000, age: 300, now: now)
        try folder(inbox, "02", bytes: 64_000, age: 200, now: now)
        inbox.trim(toBytes: 1, keeping: ["01"])
        XCTAssertEqual(names(inbox), ["01"])
    }

    func testCleanAppliesAgeAndSize() throws {
        let inbox = inbox(), now = Date()
        try folder(inbox, "01", bytes: 10, age: 10 * 86_400, now: now)
        try folder(inbox, "02", bytes: 64_000, age: 200, now: now)
        try folder(inbox, "03", bytes: 64_000, age: 100, now: now)
        inbox.clean(retentionDays: 7, maxBytes: 100_000, keeping: [], now: now)
        XCTAssertEqual(names(inbox), ["03"])
    }
}
