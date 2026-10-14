import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class ClipboardStartAfterFailedLoadTests: EngineTestCase {
    private func signedOutBridge() async throws -> EngineBridge {
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        return bridge
    }

    func testSyncAndHotkeyStartWhenALaterSettingsLoadSucceeds() async throws {
        let bridge = try await signedOutBridge()
        let settings = try XCTUnwrap(bridge.clipSettings)
        settings.readSettings = { _ in throw FsyncError.Network(msg: "settings unavailable") }

        bridge.applyAccountState(.ready)
        await eventually("failed load is reported") { bridge.clipSync?.message != nil }
        XCTAssertEqual(bridge.clipSync?.isRunning, false, "no sync on guessed settings")
        XCTAssertEqual(bridge.hotkey?.isRegistered, false)

        settings.readSettings = { try $0.settings() }
        let ok = await settings.load()

        XCTAssertTrue(ok)
        XCTAssertEqual(bridge.clipSync?.isRunning, true, "sync starts as soon as settings load")
        XCTAssertNotNil(bridge.inboxCleanup)
        XCTAssertNotEqual(bridge.clipSync?.message, "Clipboard sync is off: your settings couldn't be read.")
        await bridge.shutdown()
    }

    func testLaterLoadDoesNotStartSyncOutsideReadyPhase() async throws {
        let bridge = try await signedOutBridge()
        let settings = try XCTUnwrap(bridge.clipSettings)
        let ok = await settings.load()
        XCTAssertTrue(ok)
        XCTAssertEqual(bridge.clipSync?.isRunning, false)
        await bridge.shutdown()
    }
}
