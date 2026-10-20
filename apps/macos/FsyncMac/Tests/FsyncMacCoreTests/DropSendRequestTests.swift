import XCTest
@testable import FsyncMacCore

final class DropSendRequestTests: EngineTestCase {
    private func file(_ name: String, in dir: URL) throws -> URL {
        let url = dir.appendingPathComponent(name)
        try Data("x".utf8).write(to: url)
        return url
    }

    func testRejectsNonFileSchemesAndEmptyInput() throws {
        XCTAssertEqual(DropSendRequest.validate([URL(string: "http://example.com/a.txt")!]), .failure(.notFileURL))
        let dir = try tempDir()
        XCTAssertEqual(DropSendRequest.validate([try file("a", in: dir), URL(string: "https://x.test/b")!]), .failure(.notFileURL))
        XCTAssertEqual(DropSendRequest.validate([]), .failure(.empty))
    }

    func testResolvesSymlinksAndDropsDuplicatesKeepingOrder() throws {
        let dir = try tempDir()
        let a = try file("a.txt", in: dir), b = try file("b.txt", in: dir)
        let link = dir.appendingPathComponent("link.txt")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: a)
        let result = try DropSendRequest.validate([b, link, a, b]).get()
        XCTAssertEqual(result, [b.resolvingSymlinksInPath(), a.resolvingSymlinksInPath()])
    }

    func testMissingItemIsRejectedByNameOnly() throws {
        let dir = try tempDir()
        let gone = dir.appendingPathComponent("gone.txt")
        XCTAssertEqual(DropSendRequest.validate([try file("a", in: dir), gone]), .failure(.missing(name: "gone.txt")))
        let sub = dir.appendingPathComponent("folder")
        try FileManager.default.createDirectory(at: sub, withIntermediateDirectories: true)
        XCTAssertEqual(try DropSendRequest.validate([sub]).get().count, 1, "existing folders are fine")
    }
}
