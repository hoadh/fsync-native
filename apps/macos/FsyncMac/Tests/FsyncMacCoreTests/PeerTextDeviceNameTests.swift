import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// History rows name the sending device, never its raw node id.
final class PeerTextDeviceNameTests: XCTestCase {
    private func device(_ id: String, _ name: String) -> Device {
        Device(nodeId: id, name: name, platform: .win, status: .lan, online: true, lastSeenMs: 0, isSelf: false)
    }

    func testKnownPeerShowsDeviceName() {
        let devices = [device("4820e4fc", "WASO"), device("89b9f0bf", "alexpro")]
        XCTAssertEqual(PeerText.deviceName(peer: "4820e4fc", in: devices), "WASO")
    }

    func testUnknownOrMissingPeerNeverShowsNodeId() {
        let devices = [device("4820e4fc", "WASO")]
        XCTAssertEqual(PeerText.deviceName(peer: "ff32e2ed", in: devices), "another device")
        XCTAssertEqual(PeerText.deviceName(peer: nil, in: devices), "another device")
        XCTAssertEqual(PeerText.deviceName(peer: "4820e4fc", in: []), "another device")
    }

    func testRemoteNameIsSanitised() {
        let devices = [device("a", "Evil\u{202E}PC\n"), device("b", " \u{200B} ")]
        XCTAssertEqual(PeerText.deviceName(peer: "a", in: devices), "EvilPC")
        XCTAssertEqual(PeerText.deviceName(peer: "b", in: devices), "another device", "blank after sanitising")
    }
}
