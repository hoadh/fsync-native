import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// The real Mac logic (bridge + models, no views) against a local dev backend, with `fsync-cli` as the peer.
/// Skipped unless `FSYNC_IT_API` is set; see `apps/macos/scripts/run-fsync-mac-integration.sh`.
@MainActor
final class FsyncMacIntegrationTests: IntegrationTestCase {
    // MARK: Scenarios

    /// First device, CLI offers and the Mac joins, status goes Lan, then the CLI is removed.
    func testFirstDeviceJoinStatusAndRemove() async throws {
        let email = uniqueEmail("join")
        let bridge = try await bootMac()
        try await signIn(bridge, email: email)
        XCTAssertEqual(bridge.phase, .ready)
        XCTAssertTrue(bridge.isFirstDevice, "first device on a new account")

        let login = try await backend.cliLogin("a", email: email)
        XCTAssertEqual(login["account"] as? String, "needs_pairing")
        let cliNode = try XCTUnwrap(login["node_id"] as? String)

        let offer = try spawn("a", ["pair", "offer", "--yes"])
        let offerEvent = try await offer.waitEvent("offer")
        let code = try XCTUnwrap(offerEvent["code"] as? String)
        let pairing = try XCTUnwrap(bridge.pairingModel)
        await pairing.join(code)
        let peer = try await sasPeer(pairing)
        let sasEvent = try await offer.waitEvent("sas")
        let cliWords = try XCTUnwrap(sasEvent["words"] as? String)
        XCTAssertEqual(peer.sasWords, cliWords, "SAS words differ")
        XCTAssertEqual(peer.nodeId, cliNode)
        await pairing.confirm(requestId: peer.requestId, accept: true)
        let done = await waitUntil(timeout: 30) { if case .done = pairing.state { return true } else { return false } }
        XCTAssertTrue(done, "Mac pairing state \(pairing.state)")
        try await offer.expectExit(0, timeout: 30)
        XCTAssertNotNil(offer.lastEvent("paired"))

        // Status: the CLI stays online while it waits for a clip.
        let devices = try XCTUnwrap(bridge.devicesModel)
        let listed = await waitUntil(timeout: 10) { devices.devices.contains { $0.nodeId == cliNode } }
        XCTAssertTrue(listed, "CLI not in the device list: \(devices.devices.map(\.name))")
        let watch = try spawn("a", ["clip", "watch", "--json", "--count", "1", "--timeout", "90"])
        _ = try await watch.waitEvent("ready", timeout: 60)
        // The core reports Lan only for a live QUIC connection (none until something syncs), so "online
        // within 5 s" is the backend presence flag; the status stays offline until a connection exists.
        let started = Date()
        let online = await waitUntil(timeout: 5) { devices.devices.first { $0.nodeId == cliNode }?.online == true }
        let seconds = Date().timeIntervalSince(started)
        print("[mac-it] CLI online after \(String(format: "%.1f", seconds)) s from its ready event")
        XCTAssertTrue(online, "CLI not online within 5 s of its ready event: \(devices.devices)")
        XCTAssertGreaterThan(devices.summary.online, 0)
        XCTAssertTrue(bridge.anyPeerOnline)
        watch.terminate()

        await devices.remove(nodeId: cliNode)
        XCTAssertNil(devices.errorMessage)
        let gone = await waitUntil(timeout: 10) { !devices.devices.contains { $0.nodeId == cliNode } }
        XCTAssertTrue(gone, "removed device still listed")
        XCTAssertFalse(bridge.devicesModel?.devices.isEmpty ?? true, "self stays")
    }

    /// The CLI is the first device; the Mac needs pairing, offers a code and the CLI joins.
    func testMacOffersAndCliJoins() async throws {
        let email = uniqueEmail("offer")
        let login = try await backend.cliLogin("b", email: email)
        XCTAssertEqual(login["account"] as? String, "created")
        let bridge = try await bootMac()
        try await signIn(bridge, email: email)
        XCTAssertEqual(bridge.phase, .needsPairing)

        let pairing = try XCTUnwrap(bridge.pairingModel)
        await pairing.offer(.code)
        guard case .offering(let offer, _) = pairing.state else { return XCTFail("state \(pairing.state)") }
        let join = try spawn("b", ["pair", "join", try XCTUnwrap(offer.code), "--yes"])
        let peer = try await sasPeer(pairing)
        let sasEvent = try await join.waitEvent("sas")
        let cliWords = try XCTUnwrap(sasEvent["words"] as? String)
        XCTAssertEqual(peer.sasWords, cliWords, "SAS words differ")
        await pairing.confirm(requestId: peer.requestId, accept: true)
        try await join.expectExit(0, timeout: 60)
        let ready = await waitUntil(timeout: 30) { bridge.phase == .ready }
        XCTAssertTrue(ready, "phase \(bridge.phase), pairing \(pairing.state)")
        let devices = try XCTUnwrap(bridge.devicesModel)
        let listed = await waitUntil(timeout: 10) { devices.devices.count >= 2 }
        XCTAssertTrue(listed, "devices: \(devices.devices.map(\.name))")
    }

    /// "Lost your other devices?": the Mac becomes the only trusted device after `resetAccountTrust`.
    func testRecoveryResetsTrust() async throws {
        let email = uniqueEmail("reset")
        _ = try await backend.cliLogin("c", email: email)
        let bridge = try await bootMac()
        try await signIn(bridge, email: email)
        XCTAssertEqual(bridge.phase, .needsPairing)

        let recovery = try XCTUnwrap(bridge.recoveryModel)
        recovery.begin()
        recovery.proceed()
        await recovery.reset()
        XCTAssertNil(recovery.errorMessage)
        let ready = await waitUntil(timeout: 30) { bridge.phase == .ready }
        XCTAssertTrue(ready, "phase \(bridge.phase)")
        XCTAssertTrue(bridge.isFirstDevice)
        let devices = try XCTUnwrap(bridge.devicesModel)
        await devices.reload()
        XCTAssertEqual(devices.devices.filter { !$0.isSelf }.count, 0, "other devices after reset: \(devices.devices.map(\.name))")
    }
}
