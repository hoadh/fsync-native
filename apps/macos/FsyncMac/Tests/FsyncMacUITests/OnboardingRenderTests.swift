import AppKit
import FsyncCoreKit
import SwiftUI
import TrustResetPrompt
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

private final class QuietDelegate: EngineDelegate, @unchecked Sendable {
    func onRemoteClip(item: ClipItem) {}
    func onDeviceStatus(device: Device) {}
    func onTransfer(event: TransferEvent) {}
    func onIncomingFiles(offer: ClipItem) {}
    func onPairingEvent(event: PairingEvent) {}
    func onAccountEvent(event: AccountEvent) {}
    func onSessionChanged(tokens: SessionTokens) {}
    func onError(error: FsyncError) {}
}

@MainActor
final class OnboardingRenderTests: XCTestCase {
    private func render<V: View>(_ view: V, _ name: String) throws {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        let image = try XCTUnwrap(renderer.nsImage, name)
        XCTAssertGreaterThan(image.size.width, 100, name)
        XCTAssertGreaterThan(image.size.height, 50, name)
    }

    private func makeEngine() throws -> FsyncEngine {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let cfg = EngineConfig(
            dataDir: dir.path, apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "t", platform: .mac,
            deviceSecret: Data(repeating: 7, count: 32), historyKey: Data(repeating: 9, count: 32),
            forceRelay: false, allowInsecureHttp: true)
        return try FsyncEngine(cfg: cfg, delegate: QuietDelegate())
    }

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

    private func devices(_ engine: FsyncEngine) -> DevicesModel {
        let model = DevicesModel(engine: engine, onSignInRequired: {})
        model.merge(Device(nodeId: "s", name: "This Mac", platform: .mac, status: .offline, online: false, lastSeenMs: 0, isSelf: true))
        model.merge(Device(nodeId: "a", name: "Work PC", platform: .win, status: .lan, online: true, lastSeenMs: 0, isSelf: false))
        model.merge(Device(nodeId: "b", name: "CLI", platform: .cli, status: .offline, online: false, lastSeenMs: 1_700_000_000_000, isSelf: false))
        return model
    }

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
        try render(HistoryView(history: HistoryModel(engine: engine), sync: nil), "history empty")
        try render(AccountSettingsView(bridge: bridge).frame(width: 420, height: 160), "settings account")
    }
}
