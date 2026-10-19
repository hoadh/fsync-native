import AppKit
import FsyncCoreKit
import SwiftUI
import TrustResetPrompt
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

@MainActor
final class OnboardingRenderTests: XCTestCase {
    private func render<V: View>(_ view: V, _ name: String) throws { try RenderSupport.render(view, name) }
    private func makeEngine() throws -> FsyncEngine { try RenderSupport.makeEngine(for: self) }

    func testOnboardingViewsRender() throws {
        let engine = try makeEngine()
        // LoginView/OtpView use @FocusState, which ImageRenderer cannot host (runtime warning), so they are not rendered.
        try render(FirstDeviceView {}, "first device")
        let recovery = AccountRecoveryModel(engine: engine, onResolved: { _ in }, onSignInRequired: {})
        try render(NeedsPairingView(recovery: recovery, pairing: nil), "needs pairing")
        recovery.begin()
        try render(RecoverySheet(model: recovery), "recovery explain")
        recovery.proceed()
        try render(RecoverySheet(model: recovery), "recovery final")
    }

    private func devices(_ engine: FsyncEngine) -> DevicesModel { RenderSupport.devices(engine) }

    func testPairingViewsRender() throws {
        let engine = try makeEngine()
        let model = PairingModel(engine: engine, onPaired: { _ in }, onSignInRequired: {})
        let peer = PairingPeer(requestId: 1, nodeId: String(repeating: "cd", count: 32), name: "Phone", platform: .cli, sasWords: "alpha bravo charlie delta")
        let soon = UInt64(Date().timeIntervalSince1970 * 1000) + 300_000
        let states: [(String, PairingState)] = [
            ("idle offer", .idle), ("code", .offering(PairingOffer(qrPayload: nil, code: "ABCD1234", expiresMs: soon), .code)),
            ("qr", .offering(PairingOffer(qrPayload: "fsync://pair?x=1", code: nil, expiresMs: soon), .qr)),
            ("expired", .offering(PairingOffer(qrPayload: nil, code: "ABCD1234", expiresMs: 1), .code)),
            ("sas", .sas(peer, .offer)), ("waiting", .waitingForPeer), ("done", .done("Phone")), ("failed", .failed("Nope")),
            ("joining", .joining),
        ]
        for (name, state) in states {
            model.state = state
            try render(PairingFlowView(model: model, role: .offer), name)
        }
    }

    func testQrCodeIsSquareAndRejectsEmpty() throws {
        let image = try XCTUnwrap(QrCode.image(for: "fsync://pair?x=1"))
        XCTAssertEqual(image.width, image.height)
        XCTAssertGreaterThan(image.width, 100)
        XCTAssertNil(QrCode.image(for: ""))
    }

    func testDevicesAndMenuRender() throws {
        let engine = try makeEngine()
        let devices = devices(engine)
        let sync = ClipboardSync(engine: engine, settings: try engine.settings())
        try render(DeviceListView(devices: devices.devices, onRemove: { _ in }), "device list")
        try render(StatusHeaderView(summary: devices.summary).padding(20), "status header")
        let menu = MenuBarContentView(
            devices: devices, trust: TrustResetPromptModel(engine: engine), sync: sync, banner: "Paired with Phone.",
            openHistory: {}, openMain: {}, openSettings: {}, addDevice: {}, signOut: {})
        try render(menu, "menu")
        let transfers = TransfersModel()
        let files = FilesSync(engine: engine, fetcher: FileFetchService(engine: engine, transfers: transfers)) { false }
        files.start()
        let offer = ClipItem(itemId: "0a", from: "n", fromName: "Phone", kind: .files, tsMs: 1, preview: nil, reps: [],
                             files: [FileInfo(fileId: 0, name: "report.pdf", relPath: "report.pdf", size: 5)], totalSize: 5, push: true)
        let received = expectation(description: "offer listed")
        Task { await files.receive(offer: offer); received.fulfill() }
        wait(for: [received], timeout: 5)
        XCTAssertEqual(files.offers.count, 1)
        try render(FilesMenuSection(files: files, transfers: transfers).padding(20), "files section")
        try render(DeviceRowView(device: try XCTUnwrap(devices.devices.first), onRemove: nil, onSend: { _ in }).padding(20), "device row send")
        let pairing = PairingModel(engine: engine, onPaired: { _ in }, onSignInRequired: {})
        try render(ReadyView(devices: devices, pairing: pairing, banner: nil, signOut: {}), "ready home")
    }

    func testSettingsRender() throws {
        let bridge = EngineBridge(
            config: AppConfig(apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "t"),
            secrets: AppSecrets(), dataDir: FileManager.default.temporaryDirectory)
        try render(GeneralSettingsView(launch: LaunchAtLogin()).frame(width: 420, height: 160), "settings general")
        let engine = try makeEngine()
        let clip = ClipSettingsModel(engine: engine)
        try render(SyncSettingsView(settings: clip).frame(width: 420, height: 220), "settings sync")
        try render(PrivacySettingsView(settings: clip).frame(width: 420, height: 280), "settings privacy")
        try render(StorageSettingsView(settings: clip).frame(width: 420, height: 160), "settings storage")
        try render(HistoryView(history: HistoryModel(engine: engine), devices: DevicesModel(engine: engine) {}, sync: nil), "history empty")
        try render(AccountSettingsView(bridge: bridge).frame(width: 420, height: 160), "settings account")
    }
}
