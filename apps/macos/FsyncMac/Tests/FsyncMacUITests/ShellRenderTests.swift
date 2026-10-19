import AppKit
import Combine
import FsyncCoreKit
import SwiftUI
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

/// The main window shell, the root view per phase, and the Settings tabs. These use `NSHostingView` in an offscreen
/// window: `ImageRenderer` draws `NavigationSplitView`, `List` and grouped forms blank.
@MainActor
final class ShellRenderTests: XCTestCase {
    private func makeBridge() -> EngineBridge {
        EngineBridge(
            config: AppConfig(apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "t"),
            secrets: AppSecrets(), dataDir: FileManager.default.temporaryDirectory)
    }

    private func split(_ engine: FsyncEngine, navigation: MainNavigation) -> MainSplitView {
        let transfers = TransfersModel()
        return MainSplitView(
            navigation: navigation, devices: RenderSupport.devices(engine), files: RenderSupport.files(engine, transfers: transfers),
            transfers: transfers, history: HistoryModel(engine: engine), sync: nil,
            pairing: PairingModel(engine: engine, onPaired: { _ in }, onSignInRequired: {}), banner: "You're offline.", signOut: {})
    }

    func testMainSplitViewRendersEverySection() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        for section in MainSection.allCases {
            let navigation = MainNavigation()
            navigation.show(section)
            try RenderSupport.hostOffscreen(split(engine, navigation: navigation), "split \(section)")
        }
    }

    func testNilSelectionRendersDevicesAndSidebarHasThreeRows() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let navigation = MainNavigation()
        navigation.section = nil
        let host = try RenderSupport.hostOffscreen(split(engine, navigation: navigation), "split nil selection")
        let tables = Self.descendants(of: NSTableView.self, in: host)
        XCTAssertTrue(tables.contains { $0.numberOfRows == 3 }, "sidebar renders 3 rows; tables: \(tables.map(\.numberOfRows))")
        XCTAssertEqual(SidebarView.sections.count, 3)
        XCTAssertEqual(MainSection.allCases.map(SidebarView.title), ["Devices", "History", "Transfers"])
        XCTAssertEqual(Set(MainSection.allCases.map(SidebarView.symbol)).count, 3, "each row has its own icon")
    }

    private static func descendants<T: NSView>(of type: T.Type, in view: NSView) -> [T] {
        view.subviews.flatMap { sub in (sub as? T).map { [$0] } ?? [] + descendants(of: type, in: sub) }
    }

    func testPairingSheetFlagPresentsSheetWithoutCrashing() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let navigation = MainNavigation()
        navigation.presentPairing()
        try RenderSupport.hostOffscreen(split(engine, navigation: navigation), "split with pairing sheet")
        navigation.dismissPairing()
        XCTAssertFalse(navigation.isPairingSheetPresented)
    }

    func testStatusPillStates() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        for summary in [RenderSupport.devices(engine, others: false).summary, RenderSupport.devices(engine).summary] {
            try RenderSupport.render(StatusPill(summary: summary).padding(20), "pill \(summary.headline)")
        }
    }

    func testRootViewRendersEveryPhase() throws {
        let phases: [AppPhase] = [
            .booting, .signedOut, .needsPairing, .ready,
            .bootFailed("Disk is full", canReset: false), .bootFailed("Key mismatch", canReset: true),
        ]
        for phase in phases {
            let bridge = makeBridge()
            bridge.phase = phase
            try RenderSupport.hostOffscreen(RootView(bridge: bridge), "root \(phase)", size: CGSize(width: 620, height: 420))
        }
    }

    func testRootViewReadyWithModelsShowsSplitView() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let bridge = makeBridge()
        bridge.phase = .ready
        let transfers = TransfersModel()
        bridge.devicesModel = RenderSupport.devices(engine)
        bridge.pairingModel = PairingModel(engine: engine, onPaired: { _ in }, onSignInRequired: {})
        bridge.filesSync = RenderSupport.files(engine, transfers: transfers)
        bridge.transfers = transfers
        bridge.historyModel = HistoryModel(engine: engine)
        let host = try RenderSupport.hostOffscreen(RootView(bridge: bridge), "root ready with models", size: CGSize(width: 760, height: 520))
        let tables = Self.descendants(of: NSTableView.self, in: host)
        XCTAssertTrue(tables.contains { $0.numberOfRows == 3 }, "the sidebar is shown, not the booting view")
        bridge.isFirstDevice = true
        try RenderSupport.hostOffscreen(RootView(bridge: bridge), "root ready first device", size: CGSize(width: 620, height: 420))
    }

    func testSettingsTabsRender() throws {
        let bridge = makeBridge()
        try RenderSupport.hostOffscreen(SettingsView(bridge: bridge), "settings window", size: CGSize(width: 500, height: 380))
        let engine = try RenderSupport.makeEngine(for: self)
        let clip = ClipSettingsModel(engine: engine)
        let size = CGSize(width: 500, height: 380)
        try RenderSupport.hostOffscreen(GeneralSettingsView(launch: LaunchAtLogin()), "tab general", size: size)
        try RenderSupport.hostOffscreen(SyncSettingsView(settings: clip), "tab sync", size: size)
        try RenderSupport.hostOffscreen(PrivacySettingsView(settings: clip), "tab privacy", size: size)
        try RenderSupport.hostOffscreen(StorageSettingsView(settings: clip), "tab storage", size: size)
        bridge.phase = .signedOut
        try RenderSupport.hostOffscreen(AccountSettingsView(bridge: bridge), "tab account signed out", size: size)
        bridge.phase = .ready
        try RenderSupport.hostOffscreen(AccountSettingsView(bridge: bridge), "tab account signed in", size: size)
        bridge.phase = .bootFailed("x", canReset: false)
        try RenderSupport.hostOffscreen(AccountSettingsView(bridge: bridge), "tab account boot failed", size: size)
    }

    func testOpenerIsRecognisedByEitherIdentifier() {
        let window = NSWindow(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: true)
        XCTAssertFalse(WindowIDs.isSettingsOpener(window))
        window.identifier = WindowIDs.settingsOpener
        XCTAssertTrue(WindowIDs.isSettingsOpener(window))
        // SwiftUI puts the scene id back after the window is restyled.
        window.identifier = NSUserInterfaceItemIdentifier(WindowIDs.SceneIDs.settingsOpener)
        XCTAssertTrue(WindowIDs.isSettingsOpener(window))
        window.identifier = WindowIDs.main
        XCTAssertFalse(WindowIDs.isSettingsOpener(window))
    }

    func testOpenerWindowIsHiddenAndCannotTakeFocus() {
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 400, height: 300), styleMask: [.titled, .closable], backing: .buffered, defer: true)
        XCTAssertTrue(window.canBecomeKey)
        OpenerWindowStyle.hide(window)
        XCTAssertFalse(window.canBecomeKey)
        XCTAssertFalse(window.canBecomeMain)
        XCTAssertEqual(window.alphaValue, 0)
        XCTAssertTrue(window.ignoresMouseEvents)
        XCTAssertTrue(window.isExcludedFromWindowsMenu)
        XCTAssertTrue(window.collectionBehavior.contains(.ignoresCycle))
        XCTAssertEqual(window.frame.size, CGSize(width: 1, height: 1))
    }

    /// A request made before the opener window exists is kept and consumed once, by whoever appears first.
    func testSettingsRequestIsKeptUntilConsumed() {
        XCTAssertFalse(SettingsRequest.takePending())
        var received = 0
        let subscription = SettingsRequest.publisher.sink { received += 1 }
        SettingsRequest.send()
        SettingsRequest.send()
        XCTAssertEqual(received, 2)
        XCTAssertTrue(SettingsRequest.takePending())
        XCTAssertFalse(SettingsRequest.takePending(), "consumed once")
        subscription.cancel()
        SettingsRequest.send()  // nobody subscribed yet: still pending for the opener's onAppear
        XCTAssertTrue(SettingsRequest.takePending())
    }

    /// A request the opener never consumed in time is dropped, so a late opener does not show Settings unprompted.
    func testStaleSettingsRequestIsDropped() {
        let sent = Date()
        SettingsRequest.send(now: sent)
        XCTAssertFalse(SettingsRequest.takePending(now: sent.addingTimeInterval(SettingsRequest.maxAge + 1)))
        XCTAssertFalse(SettingsRequest.takePending(now: sent), "dropped, not kept for later")
    }
}
