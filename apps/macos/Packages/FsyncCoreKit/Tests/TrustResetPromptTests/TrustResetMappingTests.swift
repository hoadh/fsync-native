import FsyncCoreKit
import XCTest
@testable import TrustResetPrompt

final class TrustResetMappingTests: XCTestCase {
    func testAccountStateToOutcome() {
        XCTAssertEqual(TrustResetMapping.outcome(for: .ready), .trusted)
        XCTAssertEqual(TrustResetMapping.outcome(for: .created), .trusted)
        XCTAssertEqual(TrustResetMapping.outcome(for: .needsPairing), .removedNeedsPairing)
    }

    func testErrorMessages() {
        let notFound = TrustResetMapping.message(for: FsyncError.NotFound)
        XCTAssertEqual(notFound.text, "This request changed or was already answered. Review the latest one.")
        XCTAssertTrue(notFound.isNotice)

        let network = TrustResetMapping.message(for: FsyncError.Network(msg: "dns"))
        XCTAssertTrue(network.text.hasPrefix("Couldn't reach the server"))
        XCTAssertFalse(network.isNotice)
        XCTAssertFalse(network.text.contains("dns"))

        let auth = TrustResetMapping.message(for: FsyncError.Auth)
        XCTAssertEqual(auth, TrustResetMapping.message(for: FsyncError.ReauthRequired))
        XCTAssertEqual(auth.text, "Please sign in again, then retry.")
        XCTAssertFalse(auth.isNotice)

        let other = TrustResetMapping.message(for: FsyncError.Internal(msg: "secret detail"))
        XCTAssertEqual(other.text, "Something went wrong. Please try again.")
        XCTAssertFalse(other.isNotice)
        XCTAssertEqual(other, TrustResetMapping.message(for: CancellationError()))
    }

    /// The dialog closes when the shown reset changes, and never opens without an id.
    func testDialogOnlyShowsForTheResetItWasOpenedFor() {
        XCTAssertTrue(TrustResetMapping.dialogIsPresented(confirmingId: "a", displayedId: "a"))
        XCTAssertFalse(TrustResetMapping.dialogIsPresented(confirmingId: "a", displayedId: "b"))
        XCTAssertFalse(TrustResetMapping.dialogIsPresented(confirmingId: "a", displayedId: nil))
        XCTAssertFalse(TrustResetMapping.dialogIsPresented(confirmingId: nil, displayedId: "a"))
    }
}
