import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class PairingModelTests: EngineTestCase {
    private final class Calls {
        var paired: [String] = []
        var signIn = 0
    }

    private func makeModel(session: Bool) throws -> (PairingModel, Calls) {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        if session { try engine.setSession(tokens: fakeTokens) }
        let calls = Calls()
        return (PairingModel(engine: engine, onPaired: { calls.paired.append($0) }, onSignInRequired: { calls.signIn += 1 }), calls)
    }

    private func peer(_ id: UInt64, name: String = "Phone") -> PairingPeer {
        PairingPeer(requestId: id, nodeId: String(repeating: "ab", count: 32), name: name, platform: .cli, sasWords: "alpha bravo charlie")
    }

    private let offer = PairingOffer(qrPayload: nil, code: "ABCD1234", expiresMs: 1)

    /// With no account the engine answers `Invalid`, so the user sees text and can try again.
    func testOfferWithoutSessionFailsWithText() async throws {
        let (model, calls) = try makeModel(session: false)
        await model.offer(.code)
        XCTAssertEqual(model.state, .failed("That didn't work. Check it and try again."))
        XCTAssertEqual(calls.signIn, 0)
        XCTAssertFalse(model.isWorking)
    }

    func testOfferOfflineFailsWithText() async throws {
        let (model, calls) = try makeModel(session: true)
        await model.offer(.code)
        guard case .failed(let text) = model.state else { return XCTFail("state \(model.state)") }
        XCTAssertFalse(text.isEmpty)
        XCTAssertEqual(calls.signIn, 0)
        model.dismiss()
        XCTAssertEqual(model.state, .idle)
    }

    func testCancelWhileIdleIsQuiet() async throws {
        let (model, calls) = try makeModel(session: false)
        await model.cancel()
        model.openJoin()
        XCTAssertTrue(model.isActive)
        await model.cancel()
        XCTAssertEqual(model.state, .idle)
        XCTAssertFalse(model.isActive)
        XCTAssertEqual(calls.signIn, 0)
    }

    func testCancelOpenOfferEndsIt() async throws {
        let (model, _) = try makeModel(session: false)
        model.state = .offering(offer, .code)
        await model.cancel()
        XCTAssertEqual(model.state, .idle)
    }

    func testJoinIgnoresBlankInputAndFailsOffline() async throws {
        let (model, _) = try makeModel(session: true)
        await model.join("   \n")
        XCTAssertEqual(model.state, .idle)
        await model.join(" fsync://pair?x=1 ")
        guard case .failed = model.state else { return XCTFail("state \(model.state)") }
    }

    func testNewerConfirmRequestReplacesPeer() throws {
        let (model, _) = try makeModel(session: true)
        model.state = .offering(offer, .code)
        model.handle(.confirmRequest(peer: peer(1)))
        XCTAssertEqual(model.state, .sas(peer(1), .offer))
        model.handle(.confirmRequest(peer: peer(2)))
        XCTAssertEqual(model.state, .sas(peer(2), .offer))
    }

    func testConfirmRequestIsIgnoredWhenNothingIsOpen() throws {
        let (model, _) = try makeModel(session: true)
        model.handle(.confirmRequest(peer: peer(1)))
        XCTAssertEqual(model.state, .idle)
    }

    func testConfirmWithStaleIdDoesNothing() async throws {
        let (model, _) = try makeModel(session: true)
        model.state = .sas(peer(2), .offer)
        await model.confirm(requestId: 1, accept: true)
        XCTAssertEqual(model.state, .sas(peer(2), .offer))
    }

    func testRejectGoesToFailedAndNeverToDone() async throws {
        let (model, calls) = try makeModel(session: true)
        model.state = .sas(peer(5), .offer)
        await model.confirm(requestId: 5, accept: false)
        guard case .failed = model.state else { return XCTFail("state \(model.state)") }
        XCTAssertTrue(calls.paired.isEmpty)
    }

    func testAnswerInFlightIsDroppedWhenANewerRequestArrives() async throws {
        let (model, _) = try makeModel(session: true)
        model.state = .sas(peer(1), .offer)
        let answer = Task { await model.confirm(requestId: 1, accept: true) }
        await Task.yield()
        XCTAssertEqual(model.state, .waitingForPeer)
        model.handle(.confirmRequest(peer: peer(2)))
        await answer.value  // the engine rejects id 1; that result must not touch the new request
        XCTAssertEqual(model.state, .sas(peer(2), .offer))
    }

    func testCompletedFinishesAndReportsPeer() throws {
        let (model, calls) = try makeModel(session: true)
        model.state = .waitingForPeer
        model.handle(.completed(peer: "Pho\u{202E}ne\n", seq: 3))
        XCTAssertEqual(model.state, .done("Phone"))
        XCTAssertEqual(calls.paired, ["Phone"])
    }

    func testFailedEventShowsReasonOnlyWhenOpen() throws {
        let (model, _) = try makeModel(session: true)
        model.handle(.failed(reason: "boom"))
        XCTAssertEqual(model.state, .idle)
        model.state = .offering(offer, .code)
        model.handle(.failed(reason: "boom"))
        XCTAssertEqual(model.state, .failed("boom"))
    }

    /// Switching Code/QR keeps `.offering` while the new offer opens, so a SAS request is not lost.
    func testSasRequestWhileSwitchingOfferModeIsKept() async throws {
        let (model, _) = try makeModel(session: false)
        model.state = .offering(offer, .code)
        let switching = Task { await model.offer(.qr) }
        await Task.yield()
        XCTAssertEqual(model.state, .offering(offer, .code))
        XCTAssertTrue(model.isWorking)
        model.handle(.confirmRequest(peer: peer(1)))
        XCTAssertEqual(model.state, .sas(peer(1), .offer))
        await switching.value  // the new offer's result is stale and must not replace the SAS
        XCTAssertEqual(model.state, .sas(peer(1), .offer))
        XCTAssertFalse(model.isWorking)
    }

    /// Rapid Code/QR taps: the second call is ignored while the first runs.
    func testSecondOfferWhileOneIsRunningIsIgnored() async throws {
        let (model, _) = try makeModel(session: false)
        model.state = .offering(offer, .code)
        let first = Task { await model.offer(.qr) }
        await Task.yield()
        XCTAssertTrue(model.isWorking)
        await model.offer(.code)
        XCTAssertTrue(model.isWorking, "the ignored call must not reset the running one")
        XCTAssertEqual(model.state, .offering(offer, .code))
        await first.value
        XCTAssertFalse(model.isWorking)
    }

    func testLateFailureDoesNotOverwriteDoneOrFailed() throws {
        let (model, _) = try makeModel(session: true)
        model.state = .done("Phone")
        model.handle(.failed(reason: "late"))
        XCTAssertEqual(model.state, .done("Phone"))
        model.state = .failed("first")
        model.handle(.failed(reason: "second"))
        XCTAssertEqual(model.state, .failed("first"))
    }

    func testJoinerKeepsRoleOnNewRequest() throws {
        let (model, _) = try makeModel(session: true)
        model.state = .sas(peer(1), .join)
        model.handle(.confirmRequest(peer: peer(2)))
        XCTAssertEqual(model.state, .sas(peer(2), .join))
    }

    func testDoneSurvivesCancelFromViewDisappearing() async throws {
        let (model, _) = try makeModel(session: true)
        model.state = .done("Phone")
        await model.cancel()
        XCTAssertEqual(model.state, .done("Phone"))
        model.dismiss()
        XCTAssertEqual(model.state, .idle)
    }
}

final class PairingHelpersTests: XCTestCase {
    func testCodeGroupsAndExpiry() {
        XCTAssertEqual(PairingCode.grouped("abcd-efgh ij"), "abcd efgh ij")
        XCTAssertEqual(PairingCode.grouped(""), "")
        let now = Date(timeIntervalSince1970: 1000)
        XCTAssertEqual(PairingCode.secondsLeft(expiresMs: 1_000_000 + 90_500, now: now), 91)
        XCTAssertEqual(PairingCode.secondsLeft(expiresMs: 5, now: now), 0)
    }

    func testPeerTextSanitize() {
        XCTAssertEqual(PeerText.sanitize("  a\u{202E}b\nc  "), "abc")
        XCTAssertEqual(PeerText.sanitize(String(repeating: "x", count: 50)), String(repeating: "x", count: 40) + "…")
    }
}
