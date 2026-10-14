import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Silent delegate for engines the tests drive directly.
final class NullDelegate: EngineDelegate, @unchecked Sendable {
    func onRemoteClip(item: ClipItem) {}
    func onDeviceStatus(device: Device) {}
    func onTransfer(event: TransferEvent) {}
    func onIncomingFiles(offer: ClipItem) {}
    func onPairingEvent(event: PairingEvent) {}
    func onAccountEvent(event: AccountEvent) {}
    func onSessionChanged(tokens: SessionTokens) {}
    func onError(error: FsyncError) {}
}

let fakeTokens = SessionTokens(access: "a", refresh: "r", accountId: "8d5c3f0e-4c3a-4f55-9a53-2f3a1f0b9c11")

class EngineTestCase: XCTestCase {
    /// Real Keychain under a throwaway service; items are deleted in teardown.
    func makeStore() -> KeychainStore {
        let store = KeychainStore(service: "\(AppConfig.bundleId).tests.\(UUID().uuidString)")
        addTeardownBlock {
            for account in [AppSecrets.deviceSecretAccount, AppSecrets.historyKeyAccount, AppSecrets.sessionAccount] {
                try? store.delete(account)
            }
        }
        return store
    }

    func tempDir() throws -> URL {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: d) }
        return d
    }

    /// Nothing listens on port 9, so network calls fail fast with `Network`.
    var offlineConfig: AppConfig { AppConfig(apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "swift-test") }

    func engineConfig(_ dir: URL, secrets: AppSecrets) throws -> EngineConfig {
        EngineConfig(
            dataDir: dir.path, apiBase: offlineConfig.apiBase, relayUrl: nil, deviceName: "swift-test", platform: .mac,
            deviceSecret: try secrets.loadOrCreateKey(AppSecrets.deviceSecretAccount),
            historyKey: try secrets.loadOrCreateKey(AppSecrets.historyKeyAccount),
            forceRelay: false, allowInsecureHttp: true)
    }

    func makeEngine(_ dir: URL, secrets: AppSecrets) throws -> FsyncEngine {
        try FsyncEngine(cfg: try engineConfig(dir, secrets: secrets), delegate: NullDelegate())
    }

    /// Polls on the main actor until `condition` holds.
    @MainActor
    func eventually(_ what: String, timeout: Double = 10, _ condition: @escaping @MainActor () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        var ok = condition()
        while !ok && Date() < deadline {
            try? await Task.sleep(nanoseconds: 20_000_000)
            ok = condition()
        }
        XCTAssertTrue(ok, "timed out: \(what)")
    }
}
