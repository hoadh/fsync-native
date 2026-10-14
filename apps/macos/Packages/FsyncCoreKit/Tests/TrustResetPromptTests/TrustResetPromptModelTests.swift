import FsyncCoreKit
import XCTest
@testable import TrustResetPrompt

/// Silent delegate; the model is driven through the real engine API.
private final class NullDelegate: EngineDelegate, @unchecked Sendable {
    func onRemoteClip(item: ClipItem) {}
    func onDeviceStatus(device: Device) {}
    func onTransfer(event: TransferEvent) {}
    func onIncomingFiles(offer: ClipItem) {}
    func onPairingEvent(event: PairingEvent) {}
    func onAccountEvent(event: AccountEvent) {}
    func onSessionChanged(tokens: SessionTokens) {}
    func onError(error: FsyncError) {}
}

/// Model against a real engine on a fresh data dir. The happy confirm/decline path needs a
/// backend and is covered by the Rust tests (core/fsync-core/tests/service_trust_reset*.rs).
@MainActor
final class TrustResetPromptModelTests: XCTestCase {
    private func makeModel() throws -> TrustResetPromptModel {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let cfg = EngineConfig(
            dataDir: dir.path, apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "swift-test",
            platform: .mac, deviceSecret: Data(repeating: 7, count: 32),
            historyKey: Data(repeating: 9, count: 32), forceRelay: false, allowInsecureHttp: true)
        return TrustResetPromptModel(engine: try FsyncEngine(cfg: cfg, delegate: NullDelegate()))
    }

    private func settle(_ model: TrustResetPromptModel) async {
        for _ in 0..<500 where model.isBusy { try? await Task.sleep(nanoseconds: 10_000_000) }
    }

    private let stale = TrustResetInfo(resetId: String(repeating: "ab", count: 32), devices: 1, deviceNames: ["Mac"])

    func testRefreshOnFreshEngineIsEmpty() throws {
        let model = try makeModel()
        model.pending = [stale]
        model.refresh()
        XCTAssertTrue(model.pending.isEmpty)
        XCTAssertEqual(model.outcome, .unanswered)
        XCTAssertNil(model.errorMessage)
    }

    func testNothingPendingGoesToTheEngineAndReportsNotFound() async throws {
        let model = try makeModel()
        for answer in [model.confirm, model.decline] {
            answer(stale.resetId)
            await settle(model)
            XCTAssertEqual(model.errorMessage, "This request changed or was already answered. Review the latest one.")
            XCTAssertTrue(model.errorIsNotice)
            XCTAssertFalse(model.isBusy)
            XCTAssertEqual(model.outcome, .unanswered)
        }
    }

    /// A list change during the call: nothing is trusted or declined, NotFound shows the notice,
    /// and the list is refreshed from the engine. The engine knows neither A nor B, so this does
    /// not prove which id was sent; that is fixed by the `resetId:` signature (no lookup inside).
    func testListChangeDuringAnswerRefreshesAndTrustsNothing() async throws {
        let model = try makeModel()
        let a = stale
        let b = TrustResetInfo(resetId: String(repeating: "cd", count: 32), devices: 2, deviceNames: ["Mac", "Phone"])
        for answer in [model.confirm, model.decline] {
            model.pending = [a]
            answer(a.resetId)
            model.pending = [b]  // the offer grew while the tap was in flight
            await settle(model)
            XCTAssertEqual(model.outcome, .unanswered, "nothing is trusted or declined")
            XCTAssertTrue(model.errorIsNotice)
            XCTAssertTrue(model.pending.isEmpty, "NotFound refreshes from the engine")
            XCTAssertFalse(model.isBusy)
        }
    }

    func testDoubleTapIsIgnoredWhileBusy() async throws {
        let model = try makeModel()
        model.confirm(resetId: stale.resetId)
        XCTAssertTrue(model.isBusy)
        model.decline(resetId: stale.resetId)
        await settle(model)
        XCTAssertFalse(model.isBusy)
    }

    func testChangedFirstIdClearsMessageAndNoticesReplacement() throws {
        let model = try makeModel()
        let b = TrustResetInfo(resetId: String(repeating: "cd", count: 32), devices: 1, deviceNames: ["Mac"])
        model.apply([stale])
        model.apply([stale])  // same id: nothing happens
        XCTAssertNil(model.errorMessage)
        model.apply([b])  // replaced while shown
        XCTAssertEqual(model.errorMessage, TrustResetMapping.listChanged.text)
        XCTAssertTrue(model.errorIsNotice)
        model.apply([])  // gone: message cleared, no notice
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.errorIsNotice)
    }

    func testDismissOutcomeClearsMessage() throws {
        let model = try makeModel()
        model.apply([stale])
        model.apply([TrustResetInfo(resetId: "x", devices: 1, deviceNames: ["Mac"])])
        XCTAssertNotNil(model.errorMessage)
        model.dismissOutcome()
        XCTAssertNil(model.errorMessage)
    }

    func testHandleRefreshesOnlyOnTrustReset() throws {
        let model = try makeModel()
        model.pending = [stale]
        for event in [AccountEvent.selfRemoved, .sessionExpired, .listAdvanced(seq: 1), .historyReset(backup: "b")] {
            model.handle(event)
            XCTAssertEqual(model.pending, [stale], "\(event) must not refresh")
        }
        model.handle(.trustReset(resetId: stale.resetId, devices: 1, deviceNames: ["Mac"]))
        XCTAssertTrue(model.pending.isEmpty, "trustReset refreshes from the engine")
    }
}
