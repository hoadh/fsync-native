import XCTest
@testable import FsyncCoreKit

/// Records callbacks; Rust calls these from its own thread.
final class Recorder: EngineDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var accountEvents: [AccountEvent] = []
    private var callbackThreadIsMain: Bool?
    let gotAccountEvent = XCTestExpectation(description: "account event")

    func onRemoteClip(item: ClipItem) {}
    func onDeviceStatus(device: Device) {}
    func onTransfer(event: TransferEvent) {}
    func onIncomingFiles(offer: ClipItem) {}
    func onPairingEvent(event: PairingEvent) {}
    func onAccountEvent(event: AccountEvent) {
        lock.lock()
        accountEvents.append(event)
        callbackThreadIsMain = Thread.isMainThread
        lock.unlock()
        gotAccountEvent.fulfill()
    }
    func onSessionChanged(tokens: SessionTokens) {}
    func onError(error: FsyncError) {}

    var events: [AccountEvent] { lock.lock(); defer { lock.unlock() }; return accountEvents }
    var onMain: Bool? { lock.lock(); defer { lock.unlock() }; return callbackThreadIsMain }
}

final class FsyncCoreKitTests: XCTestCase {
    private func config(_ dir: URL, historyKey: UInt8) -> EngineConfig {
        EngineConfig(
            dataDir: dir.path,
            apiBase: "http://127.0.0.1:9",
            relayUrl: nil,
            deviceName: "swift-test",
            platform: .mac,
            deviceSecret: Data(repeating: 7, count: 32),
            historyKey: Data(repeating: historyKey, count: 32),
            forceRelay: false,
            allowInsecureHttp: true
        )
    }

    private func tempDir() throws -> URL {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: d) }
        return d
    }

    func testContractVersion() {
        XCTAssertEqual(contractVersion(), 1)
    }

    func testSettingsHistoryDevicesOnFreshEngine() throws {
        let engine = try FsyncEngine(cfg: config(try tempDir(), historyKey: 9), delegate: Recorder())
        XCTAssertEqual(engine.nodeId().count, 64)
        XCTAssertTrue(engine.devices().isEmpty)
        XCTAssertTrue(try engine.history(query: nil, cursor: nil, limit: 20).items.isEmpty)

        var s = try engine.settings()
        XCTAssertEqual(s.clipMode, .auto)
        s.autoAcceptFiles = true
        s.denylist = ["com.example.secrets"]
        try engine.setSettings(settings: s)
        XCTAssertEqual(try engine.settings().denylist, ["com.example.secrets"])
    }

    func testErrorsAreTyped() throws {
        let engine = try FsyncEngine(cfg: config(try tempDir(), historyKey: 9), delegate: Recorder())
        XCTAssertThrowsError(try engine.ensureAccount()) { err in
            guard case FsyncError.Auth = err else { return XCTFail("expected Auth, got \(err)") }
        }
        XCTAssertThrowsError(try engine.history(query: nil, cursor: "garbage", limit: 5)) { err in
            guard case FsyncError.Invalid = err else { return XCTFail("expected Invalid, got \(err)") }
        }
        var bad = config(try tempDir(), historyKey: 9)
        bad.historyKey = Data(repeating: 1, count: 5)
        XCTAssertThrowsError(try FsyncEngine(cfg: bad, delegate: Recorder()))
    }

    func testDelegateCallbackArrivesOffTheMainThread() throws {
        let dir = try tempDir()
        _ = try FsyncEngine(cfg: config(dir, historyKey: 9), delegate: Recorder())
        // Same data, different history key: a distinct error, nothing is moved aside.
        XCTAssertThrowsError(try FsyncEngine(cfg: config(dir, historyKey: 10), delegate: Recorder())) { err in
            guard case FsyncError.HistoryKeyMismatch = err else { return XCTFail("expected HistoryKeyMismatch, got \(err)") }
        }
        // A genuinely damaged file is reset and the app is told, off the main thread.
        try Data(repeating: 0x5A, count: 20_000).write(to: URL(fileURLWithPath: "\(dir.path)/history.db"))
        try? FileManager.default.removeItem(atPath: "\(dir.path)/history.db-wal")
        let rec = Recorder()
        let engine = try FsyncEngine(cfg: config(dir, historyKey: 9), delegate: rec)
        wait(for: [rec.gotAccountEvent], timeout: 10)
        guard case .historyReset(let backup)? = rec.events.first else {
            return XCTFail("expected historyReset, got \(rec.events)")
        }
        XCTAssertFalse(backup.isEmpty)
        XCTAssertEqual(rec.onMain, false)
        _ = engine
    }

    /// The way out of a key mismatch: the user confirms, the old history is moved aside, and the
    /// engine then opens with the new key.
    func testResetHistoryRecoversFromAKeyMismatch() throws {
        let dir = try tempDir()
        _ = try FsyncEngine(cfg: config(dir, historyKey: 9), delegate: Recorder())
        XCTAssertThrowsError(try FsyncEngine(cfg: config(dir, historyKey: 10), delegate: Recorder())) { err in
            guard case FsyncError.HistoryKeyMismatch = err else { return XCTFail("expected HistoryKeyMismatch, got \(err)") }
        }
        try resetHistory(dataDir: dir.path)
        _ = try FsyncEngine(cfg: config(dir, historyKey: 10), delegate: Recorder())
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .contains { $0.hasPrefix("history.corrupt-") }
        XCTAssertTrue(kept)
    }

    /// A 10 MiB `Data` crosses the boundary into Rust (as a config field) and comes back as a
    /// typed error, not a crash; generous bound so a debug build passes.
    func testLargeDataCrossesTheBoundary() throws {
        var cfg = config(try tempDir(), historyKey: 9)
        cfg.historyKey = Data(repeating: 0xA5, count: 10 * 1024 * 1024)
        let t0 = DispatchTime.now().uptimeNanoseconds
        XCTAssertThrowsError(try FsyncEngine(cfg: cfg, delegate: Recorder())) { err in
            guard case FsyncError.Invalid = err else { return XCTFail("expected Invalid, got \(err)") }
        }
        let ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6
        XCTAssertLessThan(ms, 2000)
    }
}
