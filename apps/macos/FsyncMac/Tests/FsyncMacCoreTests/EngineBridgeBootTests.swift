import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class EngineBridgeBootTests: EngineTestCase {
    private func makeBridge(_ store: KeychainStore, dir: URL, delays: [Double] = [60]) -> EngineBridge {
        EngineBridge(config: offlineConfig, secrets: AppSecrets(store: store), dataDir: dir, retryDelays: delays)
    }

    func testBootWithoutSessionIsSignedOut() async throws {
        let bridge = makeBridge(makeStore(), dir: try tempDir())
        XCTAssertEqual(bridge.phase, .booting)
        bridge.boot()
        await eventually("signedOut") { bridge.phase == .signedOut }
        XCTAssertNotNil(bridge.loginModel)
        await bridge.shutdown()
    }

    func testBootOfflineWithStoredSessionStaysBootingWithBanner() async throws {
        let store = makeStore()
        try AppSecrets(store: store).saveSession(fakeTokens)
        let bridge = makeBridge(store, dir: try tempDir())
        bridge.boot()
        await eventually("offline banner") { bridge.banner != nil }
        XCTAssertEqual(bridge.phase, .booting)
        XCTAssertEqual(bridge.banner, "Can't reach fsync. Check your connection.")
        XCTAssertNotNil(try AppSecrets(store: store).session(), "an offline boot keeps the session")
        await bridge.shutdown()
    }

    func testCorruptHistoryIsReportedOnMainThread() async throws {
        let dir = try tempDir()
        try Data("this is not sqlite".utf8).write(to: dir.appendingPathComponent("history.db"))
        let bridge = makeBridge(makeStore(), dir: dir)
        bridge.boot()
        await eventually("history reset event") { bridge.historyResetBackup != nil }
        XCTAssertTrue(bridge.historyResetBackup?.contains("history.corrupt") ?? false, bridge.historyResetBackup ?? "nil")
        await bridge.shutdown()
    }

    func testWrongHistoryKeyFailsBootWithReset() async throws {
        let store = makeStore(), secrets = AppSecrets(store: store), dir = try tempDir()
        do { _ = try makeEngine(dir, secrets: secrets) }  // creates history.db with the stored key
        try store.set(Data(repeating: 0xAB, count: 32), account: AppSecrets.historyKeyAccount)
        let bridge = makeBridge(store, dir: dir)
        bridge.boot()
        await eventually("bootFailed") { if case .bootFailed = bridge.phase { return true } else { return false } }
        guard case .bootFailed(_, let canReset) = bridge.phase else { return XCTFail("phase \(bridge.phase)") }
        XCTAssertTrue(canReset)

        // "Start fresh" moves the old history aside and boots with the current key.
        await bridge.startFresh()
        await eventually("signedOut after reset") { bridge.phase == .signedOut }
        await bridge.shutdown()
    }

    func testApplyAccountStateMapsPhaseAndFirstDevice() throws {
        let bridge = makeBridge(makeStore(), dir: try tempDir())
        bridge.applyAccountState(.created)
        XCTAssertEqual(bridge.phase, .ready)
        XCTAssertTrue(bridge.isFirstDevice)
        bridge.applyAccountState(.needsPairing)
        XCTAssertEqual(bridge.phase, .needsPairing)
        XCTAssertFalse(bridge.isFirstDevice)
        bridge.applyAccountState(.ready)
        XCTAssertEqual(bridge.phase, .ready)
        XCTAssertFalse(bridge.isFirstDevice)
    }

    func testSessionExpiredEventClearsSessionAndSignsOut() async throws {
        let store = makeStore()
        let secrets = AppSecrets(store: store)
        try secrets.saveSession(fakeTokens)
        let bridge = makeBridge(store, dir: try tempDir())
        bridge.applyAccountState(.ready)
        bridge.receive(.account(.sessionExpired))
        XCTAssertEqual(bridge.phase, .signedOut)
        XCTAssertNil(try secrets.session())
        bridge.receive(.account(.selfRemoved))
        XCTAssertEqual(bridge.phase, .needsPairing)
    }
}
