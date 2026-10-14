import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class ClipboardWriterTests: EngineTestCase {
    private func item(_ reps: [(String, UInt64)], kind: ClipKind = .text) -> ClipItem {
        ClipItem(itemId: UUID().uuidString.lowercased(), from: "node", fromName: "Other", kind: kind, tsMs: 1, preview: "p",
                 reps: reps.map { RepInfo(mime: $0.0, size: $0.1) }, files: [], totalSize: reps.reduce(0) { $0 + $1.1 }, push: false)
    }

    func testTextAndHtmlAreWrittenWithOriginMarker() async throws {
        let pb = scratchPasteboard()
        let data = ["text/plain": Data("hi".utf8), "text/html": Data("<i>hi</i>".utf8)]
        let writer = ClipboardWriter(pasteboard: pb) { _, idx in idx == 0 ? data["text/plain"]! : data["text/html"]! }
        let result = await writer.apply(item([("text/plain", 2), ("text/html", 9)]))
        XCTAssertNil(result.failure)
        XCTAssertEqual(pb.string(forType: .string), "hi")
        XCTAssertEqual(pb.data(forType: .html), data["text/html"])
        XCTAssertTrue(pb.types?.contains(NSPasteboard.PasteboardType(SensitiveFilter.originType)) ?? false)
        XCTAssertTrue(writer.isCurrent)
        XCTAssertEqual(writer.lastWriteCount, pb.changeCount)
    }

    func testOwnWriteIsSkippedByTheWatcher() async {
        let pb = scratchPasteboard()
        var decision: FilterDecision?
        let w = PasteboardWatcher(pasteboard: pb, frontmost: { nil }) { decision = $0.decision }
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in Data("t".utf8) }
        _ = await writer.apply(item([("text/plain", 1)]))
        w.poll()
        XCTAssertEqual(decision, .skip(.ownWrite))
    }

    func testUnknownMimeOnlyIsNothingUsable() async {
        let writer = ClipboardWriter(pasteboard: scratchPasteboard()) { _, _ in Data() }
        let result = await writer.apply(item([("application/x-foo", 3)]))
        XCTAssertEqual(result.failure, .nothingUsable)
    }

    func testUnknownMimeIsIgnoredNextToKnown() async {
        let pb = scratchPasteboard()
        let writer = ClipboardWriter(pasteboard: pb) { _, idx in Data((idx == 1 ? "ok" : "bad").utf8) }
        _ = await writer.apply(item([("application/x-foo", 3), ("text/plain", 2)]))
        XCTAssertEqual(pb.string(forType: .string), "ok")
    }

    func testFailedFetchLeavesPasteboardAlone() async {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setString("keep", forType: .string)
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in throw FsyncError.NotFound }
        let result = await writer.apply(item([("text/plain", 4)]))
        XCTAssertEqual(result.failure, .fetchFailed)
        XCTAssertEqual(pb.string(forType: .string), "keep")
        XCTAssertNil(writer.lastWriteCount)
    }

    func testLargeImageIsFetchedOnlyWhenRead() async {
        let pb = scratchPasteboard()
        let png = TestImages.png
        let calls = Counter()
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in calls.bump(); return png }
        let result = await writer.apply(item([("image/png", ClipboardWriter.eagerLimit + 1)], kind: .image))
        XCTAssertNil(result.failure)
        XCTAssertEqual(calls.value, 0, "writing must not fetch")
        XCTAssertTrue(pb.types?.contains(.png) ?? false)
        XCTAssertEqual(pb.data(forType: .png), png)
        XCTAssertEqual(calls.value, 1)
    }

    func testLazyFetchFailureGivesNoData() async {
        let pb = scratchPasteboard()
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in throw FsyncError.PeerOffline }
        _ = await writer.apply(item([("image/png", ClipboardWriter.eagerLimit + 1)], kind: .image))
        XCTAssertNil(pb.data(forType: .png))
    }

    func testClearIfOursOnlyForLazyEntryThatIsStillCurrent() async {
        let pb = scratchPasteboard()
        let writer = ClipboardWriter(pasteboard: pb) { _, idx in idx == 0 ? Data("t".utf8) : TestImages.png }
        _ = await writer.apply(item([("text/plain", 1)]))
        writer.clearIfOurs()
        XCTAssertEqual(pb.string(forType: .string), "t", "eager entries stay")

        _ = await writer.apply(item([("image/png", ClipboardWriter.eagerLimit + 1)], kind: .image))
        pb.clearContents(); pb.setString("someone else", forType: .string)  // not ours any more
        writer.clearIfOurs()
        XCTAssertEqual(pb.string(forType: .string), "someone else")

        _ = await writer.apply(item([("image/png", ClipboardWriter.eagerLimit + 1)], kind: .image))
        writer.clearIfOurs()
        XCTAssertTrue((pb.types ?? []).isEmpty, "our lazy entry is removed on quit")
    }

    func testWriteRepsIsMarkedAndSkipsUnusable() {
        let pb = scratchPasteboard()
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in Data() }
        XCTAssertNil(writer.writeReps([Rep(mime: "text/plain", data: Data("copied back".utf8)), Rep(mime: "x/unknown", data: Data([1]))]).failure)
        XCTAssertEqual(pb.string(forType: .string), "copied back")
        XCTAssertTrue(writer.isCurrent)
        XCTAssertTrue(pb.types?.contains(NSPasteboard.PasteboardType(SensitiveFilter.originType)) ?? false)
    }

    func testWriteRepsWithNothingUsableLeavesThePasteboard() {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setString("keep", forType: .string)
        let writer = ClipboardWriter(pasteboard: pb) { _, _ in Data() }
        XCTAssertEqual(writer.writeReps([]).failure, .nothingUsable)
        XCTAssertEqual(writer.writeReps([Rep(mime: "text/plain", data: Data([0xFF, 0xFE]))]).failure, .nothingUsable, "invalid UTF-8")
        XCTAssertEqual(pb.string(forType: .string), "keep")
    }
}

final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var n = 0
    func bump() { lock.lock(); n += 1; lock.unlock() }
    var value: Int { lock.lock(); defer { lock.unlock() }; return n }
}

extension Result {
    var failure: Failure? { if case .failure(let e) = self { return e }; return nil }
}
