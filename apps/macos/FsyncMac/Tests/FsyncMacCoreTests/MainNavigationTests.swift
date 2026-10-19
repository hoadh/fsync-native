import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class MainNavigationTests: EngineTestCase {
    func testStartsOnDevicesShowSetsNonNilAndResetGoesBack() {
        let nav = MainNavigation()
        XCTAssertEqual(nav.section, .devices)
        nav.show(.transfers)
        nav.presentPairing()
        XCTAssertEqual(nav.section, .transfers)
        XCTAssertTrue(nav.isPairingSheetPresented)
        nav.dismissPairing()
        XCTAssertFalse(nav.isPairingSheetPresented)
        nav.section = nil  // what a List can do when the selection is cleared
        nav.show(.history)
        XCTAssertEqual(nav.section, .history)
        nav.presentPairing()
        nav.reset()
        XCTAssertEqual(nav.section, .devices)
        XCTAssertFalse(nav.isPairingSheetPresented)
    }

    private func readyBridge() async throws -> (EngineBridge, HistoryModel) {
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        bridge.applyAccountState(.ready)
        return (bridge, try XCTUnwrap(bridge.historyModel))
    }

    func testEveryShowOfHistoryRefreshesItAndOtherPagesDoNot() async throws {
        let (bridge, history) = try await readyBridge()
        await eventually("settled") { !history.isLoading }
        var generation = history.generation
        bridge.navigation.show(.history)
        await eventually("first refresh") { history.generation > generation }
        await eventually("settled") { !history.isLoading }
        generation = history.generation
        bridge.navigation.show(.history)
        await eventually("repeat refresh") { history.generation > generation }
        await eventually("settled") { !history.isLoading }
        generation = history.generation
        bridge.navigation.show(.devices)
        bridge.navigation.show(.transfers)
        try await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertEqual(history.generation, generation)
        await bridge.shutdown()
    }

    func testSignOutAndLeavingReadyResetNavigation() async throws {
        let (bridge, _) = try await readyBridge()
        bridge.navigation.show(.transfers)
        bridge.navigation.presentPairing()
        bridge.requireSignIn()
        XCTAssertEqual(bridge.navigation.section, .devices)
        XCTAssertFalse(bridge.navigation.isPairingSheetPresented)
        bridge.applyAccountState(.ready)
        bridge.navigation.show(.history)
        bridge.applyAccountState(.needsPairing)
        XCTAssertEqual(bridge.navigation.section, .devices)
        await bridge.shutdown()
    }
}
