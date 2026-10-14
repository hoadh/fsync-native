import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

private func device(_ id: String, _ name: String, _ status: DeviceStatus = .offline, isSelf: Bool = false) -> Device {
    Device(nodeId: id, name: name, platform: .cli, status: status, online: status != .offline, lastSeenMs: 0, isSelf: isSelf)
}

final class StatusSummaryTests: XCTestCase {
    /// Seen online by the backend, no QUIC connection yet: not "offline", counts as reachable.
    func testPresenceOnlyDeviceIsOnlineNotOffline() {
        let seen = Device(nodeId: "p", name: "P", platform: .cli, status: .offline, online: true, lastSeenMs: 0, isSelf: false)
        let s = StatusSummary([seen, device("o", "O")])
        XCTAssertEqual([s.present, s.offline, s.online, s.total], [1, 1, 1, 2])
        XCTAssertEqual(s.text, "0 on LAN · 0 via relay · 1 online · 1 offline")
    }

    func testHeadlineSaysConnectedOnlyForALiveLink() {
        let seen = Device(nodeId: "p", name: "P", platform: .cli, status: .offline, online: true, lastSeenMs: 0, isSelf: false)
        XCTAssertEqual(StatusSummary([]).headline, "No devices online")
        XCTAssertEqual(StatusSummary([device("o", "O")]).headline, "No devices online")
        XCTAssertEqual(StatusSummary([seen]).headline, "Online")
        XCTAssertEqual(StatusSummary([seen, device("r", "R", .relay)]).headline, "Connected")
        XCTAssertEqual(StatusSummary([device("l", "L", .lan)]).headline, "Connected")
    }

    func testCountsExcludeSelf() {
        let s = StatusSummary([
            device("a", "Me", .lan, isSelf: true), device("b", "B", .lan), device("c", "C", .relay),
            device("d", "D"), device("e", "E"),
        ])
        XCTAssertEqual(s, StatusSummary([device("b", "B", .lan), device("c", "C", .relay), device("d", "D"), device("e", "E")]))
        XCTAssertEqual([s.lan, s.relay, s.offline, s.online, s.total], [1, 1, 2, 2, 4])
        XCTAssertEqual(s.text, "1 on LAN · 1 via relay · 2 offline")
        XCTAssertEqual(StatusSummary([]).text, "No other devices yet")
    }
}

@MainActor
final class DevicesModelTests: EngineTestCase {
    private func makeModel(session: Bool = false) throws -> (DevicesModel, () -> Int) {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        if session { try engine.setSession(tokens: fakeTokens) }
        let box = Box()
        return (DevicesModel(engine: engine, onSignInRequired: { box.signIn += 1 }), { box.signIn })
    }

    private final class Box { var signIn = 0 }

    func testMergeUpdatesExistingAndAppendsUnknown() throws {
        let (model, _) = try makeModel()
        model.merge(device("b", "Bravo"))
        model.merge(device("a", "Alpha"))
        model.merge(device("b", "Bravo", .lan))
        XCTAssertEqual(model.devices.map(\.nodeId), ["a", "b"])
        XCTAssertEqual(model.devices[1].status, .lan)
        XCTAssertEqual(model.summary.lan, 1)
    }

    func testSelfSortsFirstThenByName() throws {
        let (model, _) = try makeModel()
        model.merge(device("c", "carol"))
        model.merge(device("b", "Bob"))
        model.merge(device("z", "Zed", isSelf: true))
        XCTAssertEqual(model.devices.map(\.name), ["Zed", "Bob", "carol"])
    }

    func testReloadOnFreshEngineKeepsOnlyEngineView() async throws {
        let (model, _) = try makeModel()
        await model.reload()
        XCTAssertTrue(model.devices.allSatisfy { $0.isSelf })
    }

    func testSelfAndUnknownCannotBeRemoved() async throws {
        let (model, signIn) = try makeModel()
        model.merge(device("z", "Zed", isSelf: true))
        await model.remove(nodeId: "z")
        await model.remove(nodeId: "missing")
        XCTAssertEqual(model.devices.count, 1)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(signIn(), 0)
    }

    func testRemoveOfflineShowsErrorAndKeepsList() async throws {
        let (model, signIn) = try makeModel(session: true)
        model.merge(device("b", "Bob"))
        await model.remove(nodeId: "b")
        XCTAssertNotNil(model.errorMessage)
        XCTAssertEqual(model.devices.map(\.nodeId), ["b"])
        XCTAssertFalse(model.isBusy)
        XCTAssertEqual(signIn(), 0)
    }

    /// With no account the engine answers `Invalid` (not `Auth`): the user sees text, the list stays.
    func testRemoveWithoutAccountShowsErrorAndKeepsList() async throws {
        let (model, signIn) = try makeModel()
        model.merge(device("b", "Bob"))
        await model.remove(nodeId: "b")
        XCTAssertNotNil(model.errorMessage)
        XCTAssertEqual(signIn(), 0)
        XCTAssertEqual(model.devices.count, 1)
    }
}
