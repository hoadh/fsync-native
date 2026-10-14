import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class AccountRecoveryModelTests: EngineTestCase {
    private final class Calls {
        var resolved: [AccountState] = []
        var signInRequired = 0
    }

    private func makeModel(session: Bool) throws -> (AccountRecoveryModel, Calls) {
        let secrets = AppSecrets(store: makeStore())
        let engine = try makeEngine(try tempDir(), secrets: secrets)
        if session { try engine.setSession(tokens: fakeTokens) }
        let calls = Calls()
        let model = AccountRecoveryModel(
            engine: engine, onResolved: { calls.resolved.append($0) }, onSignInRequired: { calls.signInRequired += 1 })
        return (model, calls)
    }

    func testDestructiveCallNeedsBothSteps() async throws {
        let (model, calls) = try makeModel(session: false)
        await model.reset()  // idle
        model.begin()
        XCTAssertEqual(model.step, .explain)
        await model.reset()  // explain only
        model.cancel()
        XCTAssertEqual(model.step, .idle)
        model.proceed()  // ignored from idle
        XCTAssertEqual(model.step, .idle)
        // No session: any engine call would have ended in `onSignInRequired` (Auth).
        XCTAssertEqual(calls.signInRequired, 0)
        XCTAssertTrue(calls.resolved.isEmpty)
    }

    func testNoSessionRoutesToSignIn() async throws {
        let (model, calls) = try makeModel(session: false)
        model.begin()
        model.proceed()
        XCTAssertEqual(model.step, .finalConfirm)
        await model.reset()
        XCTAssertEqual(calls.signInRequired, 1)
        XCTAssertTrue(calls.resolved.isEmpty)
        XCTAssertEqual(model.step, .idle)
        XCTAssertFalse(model.isBusy)
    }

    func testOfflineShowsErrorAndStaysOnFinalStep() async throws {
        let (model, calls) = try makeModel(session: true)
        model.begin()
        model.proceed()
        await model.reset()
        XCTAssertNotNil(model.errorMessage)
        XCTAssertEqual(model.step, .finalConfirm)
        XCTAssertFalse(model.isBusy)
        XCTAssertTrue(calls.resolved.isEmpty)
        model.cancel()
        XCTAssertNil(model.errorMessage)
    }

    /// The recovery result goes through the same AccountState mapping as boot and sign-in.
    func testResultUsesTheBridgeMapping() async throws {
        let bridge = EngineBridge(
            config: offlineConfig, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        let model = try XCTUnwrap(bridge.recoveryModel)
        bridge.applyAccountState(.needsPairing)
        XCTAssertEqual(bridge.phase, .needsPairing)

        // Created is what a successful reset returns: Mac becomes the first device.
        bridge.applyAccountState(.created)
        XCTAssertEqual(bridge.phase, .ready)
        XCTAssertTrue(bridge.isFirstDevice)

        // Wiring: no session, so the engine says Auth and the bridge asks for sign-in.
        bridge.applyAccountState(.needsPairing)
        model.begin()
        model.proceed()
        await model.reset()
        XCTAssertEqual(bridge.phase, .signedOut)
        XCTAssertEqual(bridge.banner, "Sign in again to continue.")
        await bridge.shutdown()
    }
}
