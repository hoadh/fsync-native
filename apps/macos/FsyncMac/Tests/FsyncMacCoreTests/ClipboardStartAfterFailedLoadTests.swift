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

    /// A ready bridge whose initial settings load failed (sync off, message shown).
    private func readyBridgeWithFailedLoad() async throws -> (EngineBridge, ClipSettingsModel) {
        let bridge = try await signedOutBridge()
        let settings = try XCTUnwrap(bridge.clipSettings)
        settings.readSettings = { _ in throw FsyncError.Network(msg: "settings unavailable") }
        bridge.applyAccountState(.ready)
        await eventually("failed load is reported") { bridge.clipSync?.message == EngineBridge.settingsOffMessage }
        XCTAssertEqual(bridge.clipSync?.isRunning, false, "no sync on guessed settings")
        XCTAssertEqual(bridge.hotkey?.isRegistered, false)
        settings.readSettings = { try $0.settings() }
        return (bridge, settings)
    }

    func testSyncAndHotkeyStartWhenALaterSettingsLoadSucceeds() async throws {
        let (bridge, settings) = try await readyBridgeWithFailedLoad()
        let ok = await settings.load()

        XCTAssertTrue(ok)
        XCTAssertEqual(bridge.clipSync?.isRunning, true, "sync starts as soon as settings load")
        XCTAssertNotNil(bridge.inboxCleanup)
        XCTAssertNotEqual(bridge.clipSync?.message, EngineBridge.settingsOffMessage)
        if bridge.hotkey?.isRegistered != true {
            await bridge.shutdown()
            throw XCTSkip("\(HotkeyManager.sendClipboardLabel) is owned by another app")
        }
        await bridge.shutdown()
    }

    func testOnlyTheSettingsOffMessageIsClearedOnStart() async throws {
        let (bridge, settings) = try await readyBridgeWithFailedLoad()
        bridge.clipSync?.report("Some other notice")
        await settings.load()
        XCTAssertEqual(bridge.clipSync?.isRunning, true)
        XCTAssertTrue(bridge.clipSync?.message == "Some other notice"
                      || bridge.clipSync?.message?.contains("used by another app") == true, "unrelated message kept")
        await bridge.shutdown()
    }

    func testNothingStartsAfterPrepareToQuit() async throws {
        let (bridge, settings) = try await readyBridgeWithFailedLoad()
        bridge.prepareToQuit()
        let ok = await settings.load()

        XCTAssertTrue(ok)
        XCTAssertEqual(bridge.clipSync?.isRunning, false, "a late load must not start sync while quitting")
        XCTAssertEqual(bridge.hotkey?.isRegistered, false)
        await bridge.shutdown()
    }

    func testPrepareToQuitStopsRunningSync() async throws {
        let (bridge, settings) = try await readyBridgeWithFailedLoad()
        await settings.load()
        XCTAssertEqual(bridge.clipSync?.isRunning, true)
        bridge.prepareToQuit()
        XCTAssertEqual(bridge.clipSync?.isRunning, false)
        XCTAssertEqual(bridge.hotkey?.isRegistered, false)
        await bridge.shutdown()
    }

    func testNothingStartsAfterSignOut() async throws {
        let (bridge, settings) = try await readyBridgeWithFailedLoad()
        bridge.requireSignIn(banner: nil)
        await bridge.sessionClear?.value
        XCTAssertEqual(bridge.phase, .signedOut)

        let ok = await settings.load()  // a stale in-flight load finishing after sign-out
        XCTAssertTrue(ok)
        XCTAssertEqual(bridge.clipSync?.isRunning, false)
        XCTAssertEqual(bridge.hotkey?.isRegistered, false)
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
