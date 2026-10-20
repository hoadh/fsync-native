import XCTest
@testable import FsyncMacCore

final class IncomingFilesNoticeTests: XCTestCase {
    private func notice(_ from: String = "Phone", files: Int = 2, bytes: UInt64 = 2048, auto: Bool = false) -> IncomingFilesNotice {
        IncomingFilesNotice(fromName: from, fileCount: files, totalBytes: bytes, autoAccepted: auto)
    }

    // MARK: copy

    func testTitleAndBodyFollowTheSpec() {
        XCTAssertEqual(notice().title, "Files from Phone")
        XCTAssertTrue(notice(auto: true).body.hasPrefix("2 files, "))
        XCTAssertTrue(notice(auto: true).body.hasSuffix(". Saving to Downloads"))
        XCTAssertTrue(notice(auto: false).body.hasSuffix(". Open fsync to accept"))
        XCTAssertTrue(notice(files: 1).body.hasPrefix("1 file, "))
        var merged = notice()
        merged.offerCount = 3
        XCTAssertEqual(merged.body, "3 offers from Phone")
    }

    func testSenderIsSanitizedAndEmptyBecomesAnotherDevice() {
        XCTAssertEqual(notice("gp\u{202E}j.exe").fromName, "gpj.exe")
        XCTAssertEqual(notice("\u{0007}\u{200B}\n").fromName, "another device")
        XCTAssertEqual(notice("\u{0007}\u{200B}\n").title, "Files from another device")
        XCTAssertEqual(notice(String(repeating: "a", count: 41)).fromName, String(repeating: "a", count: 40) + "…")
    }

    // MARK: sanitizer vectors (spec section 5)

    func testSanitizerVectors() {
        let a40 = String(repeating: "a", count: 40)
        let thumb = "👍🏽"
        let vectors: [(String, String)] = [
            ("Alice\nBob", "AliceBob"), ("\u{202E}evil", "evil"), ("gp\u{202E}j.exe", "gpj.exe"), ("  Mac  ", "Mac"),
            ("a\u{200D}b\u{200B}c", "abc"), (a40, a40), (a40 + "a", a40 + "…"),
            (String(repeating: "a", count: 39) + " b", String(repeating: "a", count: 39) + " …"),
            ("\u{0007}\u{200B}\n", ""), ("a  b", "a  b"),
            (String(repeating: thumb, count: 41), String(repeating: thumb, count: 40) + "…"),
        ]
        for (i, v) in vectors.enumerated() { XCTAssertEqual(PeerText.sanitize(v.0), v.1, "vector \(i + 1)") }
    }

    // MARK: coalescer (fake clock = explicit dates)

    private let t0 = Date(timeIntervalSince1970: 1000)

    func testFirstNoticePassesAndOthersInTheWindowAreFoldedIntoOne() {
        var c = IncomingFilesNoticeCoalescer()
        XCTAssertEqual(c.submit(notice(), at: t0), notice())
        XCTAssertNil(c.submit(notice(files: 1, bytes: 10), at: t0.addingTimeInterval(2)))
        XCTAssertNil(c.submit(notice(files: 3, bytes: 20), at: t0.addingTimeInterval(9.9)))
        XCTAssertEqual(c.nextDeadline, t0.addingTimeInterval(10))
        XCTAssertTrue(c.flush(at: t0.addingTimeInterval(9.9)).isEmpty)
        let out = c.flush(at: t0.addingTimeInterval(10))
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].offerCount, 2, "held offers only; the shown one is not counted")
        XCTAssertEqual(out[0].fileCount, 4)
        XCTAssertEqual(out[0].totalBytes, 30)
        XCTAssertEqual(out[0].body, "2 offers from Phone")
        XCTAssertNil(c.nextDeadline)
    }

    func testSendersAreIndependentAndWindowRestartsAfterFlushAndAfterQuiet() {
        var c = IncomingFilesNoticeCoalescer()
        XCTAssertNotNil(c.submit(notice("A"), at: t0))
        XCTAssertNotNil(c.submit(notice("B"), at: t0.addingTimeInterval(1)))
        XCTAssertNil(c.submit(notice("A"), at: t0.addingTimeInterval(3)))
        XCTAssertEqual(c.flush(at: t0.addingTimeInterval(10)).map(\.fromName), ["A"])
        // the flush opened a new window for A
        XCTAssertNil(c.submit(notice("A"), at: t0.addingTimeInterval(12)))
        XCTAssertEqual(c.flush(at: t0.addingTimeInterval(20)).first?.offerCount, 1)
        // quiet for a whole window: next offer passes at once
        XCTAssertNotNil(c.submit(notice("B"), at: t0.addingTimeInterval(60)))
    }

    func testLateTimerFoldsOfferIntoDueNoticeInsteadOfDroppingIt() {
        var c = IncomingFilesNoticeCoalescer()
        XCTAssertNotNil(c.submit(notice(), at: t0))
        XCTAssertNil(c.submit(notice(files: 1, bytes: 10), at: t0.addingTimeInterval(2)))
        // the window ended at t0+10 but flush has not run yet
        XCTAssertNil(c.submit(notice(files: 2, bytes: 5), at: t0.addingTimeInterval(11)))
        let out = c.flush(at: t0.addingTimeInterval(11))
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].offerCount, 2)
        XCTAssertEqual(out[0].fileCount, 3)
        XCTAssertEqual(out[0].totalBytes, 15)
    }
}
