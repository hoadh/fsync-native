import XCTest
@testable import TrustResetPrompt

/// Spec section 5 vectors (same table as PeerText tests in FsyncMac).
final class TrustResetDisplayNamesTests: XCTestCase {
    private func clean(_ s: String) -> String { TrustResetMapping.displayNames([s])[0] }

    func testSpecVectors() {
        let a40 = String(repeating: "a", count: 40)
        XCTAssertEqual(clean("Alice\nBob"), "AliceBob")
        XCTAssertEqual(clean("\u{202E}evil"), "evil")
        XCTAssertEqual(clean("gp\u{202E}j.exe"), "gpj.exe")
        XCTAssertEqual(clean("  Mac  "), "Mac")
        XCTAssertEqual(clean("a\u{200D}b\u{200B}c"), "abc")
        XCTAssertEqual(clean(a40), a40)
        XCTAssertEqual(clean(a40 + "a"), a40 + "…")
        let spaced = String(repeating: "a", count: 39) + " b"
        XCTAssertEqual(clean(spaced), String(repeating: "a", count: 39) + " …")
        XCTAssertEqual(clean("a  b"), "a  b")
        let thumbs = "👍🏽"
        XCTAssertEqual(clean(String(repeating: thumbs, count: 41)), String(repeating: thumbs, count: 40) + "…")
    }

    func testEmptyAfterSanitizingFallsBack() {
        XCTAssertEqual(clean("\u{0007}\u{200B}\n"), "another device")
        XCTAssertEqual(clean(""), "another device")
    }

    func testKeepsOrderAndCount() {
        XCTAssertEqual(TrustResetMapping.displayNames(["B", "\u{202E}A", "C"]), ["B", "A", "C"])
        XCTAssertEqual(TrustResetMapping.displayNames([]), [])
    }
}
