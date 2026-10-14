import Combine
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore
@testable import TrustResetPrompt

private final class SeenFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var flag: Bool?
    var value: Bool? { lock.withLock { flag } }
    func set(_ v: Bool) { lock.withLock { flag = v } }
}

@MainActor
final class EngineBridgeEventsTests: EngineTestCase {
    private func bootedBridge(store: KeychainStore? = nil) async throws -> (EngineBridge, AppSecrets) {
        let secrets = AppSecrets(store: store ?? makeStore())
        let bridge = EngineBridge(config: offlineConfig, secrets: secrets, dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        return (bridge, secrets)
    }

    private func dev(_ status: DeviceStatus) -> Device {
        Device(nodeId: "n1", name: "Peer", platform: .cli, status: status, online: status != .offline, lastSeenMs: 0, isSelf: false)
    }

    func testSelfRemovedNeedsPairing() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.receive(.account(.selfRemoved))
        XCTAssertEqual(bridge.phase, .needsPairing)
        await bridge.shutdown()
    }

    func testSessionExpiredClearsKeychainAndSignsOut() async throws {
        let (bridge, secrets) = try await bootedBridge()
        try secrets.saveSession(fakeTokens)
        bridge.applyAccountState(.ready)
        bridge.receive(.account(.sessionExpired))
        XCTAssertEqual(bridge.phase, .signedOut)
        await bridge.sessionClear?.value  // the Keychain delete runs off the main actor
        XCTAssertNil(try secrets.session())
        XCTAssertNotNil(bridge.banner)
        await bridge.shutdown()
    }

    func testSignOutClearsKeychainEvenWhenServerIsUnreachable() async throws {
        let (bridge, secrets) = try await bootedBridge()
        try secrets.saveSession(fakeTokens)
        bridge.applyAccountState(.ready)
        await bridge.signOut()
        XCTAssertEqual(bridge.phase, .signedOut)
        XCTAssertNil(try secrets.session())
        XCTAssertNil(bridge.banner)
        await bridge.shutdown()
    }

    func testTrustResetRefreshesModelAndRequestsWindow() async throws {
        let (bridge, _) = try await bootedBridge()
        var requests = 0
        let sub = bridge.windowRequest.sink { requests += 1 }
        bridge.receive(.account(.trustReset(resetId: String(repeating: "0", count: 64), devices: 1, deviceNames: ["x"])))
        XCTAssertEqual(requests, 1)
        XCTAssertEqual(bridge.trustModel?.pending.isEmpty, true, "a fresh engine has no pending reset")
        sub.cancel()
        await bridge.shutdown()
    }

    /// Drives the real `trust.$outcome` subscription; dropping the sink makes this fail.
    func testTrustOutcomeSinkRoutesToPairing() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.applyAccountState(.ready)
        bridge.pairingModel?.state = .failed("old")
        bridge.devicesModel?.merge(dev(.lan))
        bridge.trustModel?.outcome = .trusted
        XCTAssertEqual(bridge.phase, .ready)
        bridge.trustModel?.outcome = .removedNeedsPairing
        XCTAssertEqual(bridge.phase, .needsPairing)
        XCTAssertEqual(bridge.pairingModel?.state, .idle)
        XCTAssertEqual(bridge.devicesModel?.devices.isEmpty, true)
        await bridge.shutdown()
    }

    func testSelfRemovedDismissesPairingAndClearsDevices() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.applyAccountState(.ready)
        bridge.pairingModel?.state = .done("Phone")
        bridge.devicesModel?.merge(dev(.relay))
        bridge.receive(.account(.selfRemoved))
        XCTAssertEqual(bridge.phase, .needsPairing)
        XCTAssertEqual(bridge.pairingModel?.state, .idle)
        XCTAssertEqual(bridge.devicesModel?.devices.isEmpty, true)
        await bridge.shutdown()
    }

    /// A boot or sign-in check that is still running when the user signs out must not write anything.
    func testSignOutDuringAccountCheckLeavesSignedOutAndNoBanner() async throws {
        let store = makeStore()
        try AppSecrets(store: store).saveSession(fakeTokens)
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: store), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("offline banner") { bridge.banner != nil }
        bridge.didSignIn()
        await bridge.signOut()
        try await Task.sleep(for: .seconds(1))
        XCTAssertEqual(bridge.phase, .signedOut)
        XCTAssertNil(bridge.banner)
        await bridge.shutdown()
    }

    /// `start()` may refresh the token: the gate must already be open then, or the rotated pair is lost.
    func testSessionGateIsOpenBeforeEngineStart() async throws {
        let store = makeStore()
        try AppSecrets(store: store).saveSession(fakeTokens)
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: store), dataDir: try tempDir(), retryDelays: [60])
        let gate = bridge.sessionGate
        let seen = SeenFlag()
        bridge.beforeEngineStart = { seen.set(gate.isOpen) }
        bridge.boot()
        await eventually("offline banner") { bridge.banner != nil }
        XCTAssertEqual(seen.value, true)
        await bridge.shutdown()
    }

    func testGateStaysClosedWithoutStoredSession() async throws {
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), retryDelays: [60])
        let gate = bridge.sessionGate
        let seen = SeenFlag()
        bridge.beforeEngineStart = { seen.set(gate.isOpen) }
        bridge.boot()
        await eventually("signed out") { bridge.phase == .signedOut }
        XCTAssertEqual(seen.value, false)
        await bridge.shutdown()
    }

    func testSessionGateFollowsSignInAndSignOut() async throws {
        let store = makeStore()
        try AppSecrets(store: store).saveSession(fakeTokens)
        let bridge = EngineBridge(config: offlineConfig, secrets: AppSecrets(store: store), dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("offline banner") { bridge.banner != nil }
        XCTAssertTrue(bridge.sessionGate.isOpen, "a stored session lets refreshed tokens through")
        await bridge.signOut()
        XCTAssertFalse(bridge.sessionGate.isOpen)
        bridge.didSignIn()
        XCTAssertTrue(bridge.sessionGate.isOpen)
        await bridge.signOut()
        XCTAssertFalse(bridge.sessionGate.isOpen)
        await bridge.shutdown()
    }

    func testAdapterIgnoresSessionCallbackWhileGateIsClosed() throws {
        let secrets = AppSecrets(store: makeStore())
        let gate = SessionGate()
        let adapter = EngineDelegateAdapter(secrets: secrets, gate: gate, forward: { _ in })
        adapter.onSessionChanged(tokens: fakeTokens)
        XCTAssertNil(try secrets.session(), "signed out: a late refresh must not bring the session back")
        gate.set(open: true)
        adapter.onSessionChanged(tokens: fakeTokens)
        XCTAssertEqual(try secrets.session(), fakeTokens)
    }

    func testRemovedNeedsPairingOutcomeRoutesToPairing() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.applyAccountState(.ready)
        bridge.devicesModel?.merge(dev(.lan))
        bridge.handleTrustOutcome(.trusted)
        XCTAssertEqual(bridge.phase, .ready)
        bridge.handleTrustOutcome(.removedNeedsPairing)
        XCTAssertEqual(bridge.phase, .needsPairing)
        XCTAssertEqual(bridge.devicesModel?.devices.isEmpty, true)
        await bridge.shutdown()
    }

    func testDeviceStatusMergesAndDrivesIconFlag() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.receive(.deviceStatus(dev(.lan)))
        await eventually("online flag") { bridge.anyPeerOnline }
        bridge.receive(.deviceStatus(dev(.offline)))
        await eventually("offline flag") { !bridge.anyPeerOnline }
        await bridge.shutdown()
    }

    func testPairingEventsReachTheModelAndRefreshAfterCompletion() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.applyAccountState(.ready)
        bridge.pairingModel?.state = .waitingForPeer
        bridge.receive(.pairing(.completed(peer: "Peer", seq: 1)))
        XCTAssertEqual(bridge.pairingModel?.state, .done("Peer"))
        XCTAssertEqual(bridge.banner, "Paired with Peer.")
        await bridge.shutdown()
    }

    func testErrorsBecomeBannersAndAuthSignsOut() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.applyAccountState(.ready)
        bridge.receive(.error(.Network(msg: "x")))
        XCTAssertEqual(bridge.phase, .ready)
        XCTAssertEqual(bridge.banner, "Can't reach fsync. Check your connection.")
        bridge.receive(.error(.Cancelled))
        XCTAssertEqual(bridge.banner, "Can't reach fsync. Check your connection.")
        bridge.receive(.error(.Auth))
        XCTAssertEqual(bridge.phase, .signedOut)
        await bridge.shutdown()
    }

    func testHistoryResetShowsInformationalBanner() async throws {
        let (bridge, _) = try await bootedBridge()
        bridge.receive(.account(.historyReset(backup: "/tmp/h.corrupt")))
        XCTAssertEqual(bridge.historyResetBackup, "/tmp/h.corrupt")
        XCTAssertNotNil(bridge.banner)
        await bridge.shutdown()
    }
}
