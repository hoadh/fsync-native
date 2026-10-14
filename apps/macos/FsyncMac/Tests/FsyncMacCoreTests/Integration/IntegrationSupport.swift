import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Shared setup for tests against the local dev backend, with `fsync-cli` as the peer.
@MainActor
class IntegrationTestCase: EngineTestCase {
    var backend: DevBackend!
    var procs: [CliProcess] = []

    override func setUp() async throws {
        try await super.setUp()
        backend = try DevBackend.fromEnvironment()
    }

    override func tearDown() async throws {
        procs.forEach { $0.terminate() }
        procs = []
        try await super.tearDown()
    }

    // MARK: Helpers

    func spawn(_ name: String, _ args: [String]) throws -> CliProcess {
        let p = try backend.cli(name, args)
        procs.append(p)
        return p
    }

    func uniqueEmail(_ tag: String) -> String { "mac-it-\(tag)-\(UUID().uuidString.prefix(8).lowercased())@example.test".lowercased() }

    /// A booted bridge on a throwaway Keychain service and data dir, stopped at teardown.
    func bootMac(pasteboard: NSPasteboard = .general) async throws -> EngineBridge {
        let config = AppConfig(apiBase: backend.api, relayUrl: backend.relay, deviceName: "mac-it")
        let bridge = EngineBridge(config: config, secrets: AppSecrets(store: makeStore()), dataDir: try tempDir(), pasteboard: pasteboard)
        addTeardownBlock { await bridge.shutdown() }
        bridge.boot()
        let ok = await waitUntil(timeout: 30) { bridge.phase == .signedOut }
        XCTAssertTrue(ok, "boot phase \(bridge.phase)")
        return bridge
    }

    /// Email, OTP from the backend log, verify. Returns once the account state is resolved.
    func signIn(_ bridge: EngineBridge, email: String) async throws {
        let login = try XCTUnwrap(bridge.loginModel)
        login.email = email
        await login.sendCode()
        XCTAssertNil(login.errorMessage)
        login.code = try await backend.otp(for: email)
        await login.verify()
        XCTAssertNil(login.errorMessage)
        let settled = await waitUntil(timeout: 30) { bridge.phase == .ready || bridge.phase == .needsPairing }
        XCTAssertTrue(settled, "phase after sign-in: \(bridge.phase)")
    }

    func sasPeer(_ model: PairingModel, timeout: Double = 30) async throws -> PairingPeer {
        let ok = await waitUntil(timeout: timeout) { if case .sas = model.state { return true } else { return false } }
        guard ok, case .sas(let peer, _) = model.state else { throw HarnessError.timeout("Mac SAS, state \(model.state)") }
        return peer
    }

    /// A Mac that is `.ready` and paired with a CLI device `name` (the CLI offers, the Mac joins).
    /// The CLI is not running afterwards; start `clip watch` / `clip send` as needed.
    func pairedMac(cli name: String, pasteboard: NSPasteboard) async throws -> EngineBridge {
        let email = uniqueEmail(name)
        let bridge = try await bootMac(pasteboard: pasteboard)
        try await signIn(bridge, email: email)
        XCTAssertEqual(bridge.phase, .ready)
        _ = try await backend.cliLogin(name, email: email)
        let offer = try spawn(name, ["pair", "offer", "--yes"])
        let offerEvent = try await offer.waitEvent("offer")
        let code = try XCTUnwrap(offerEvent["code"] as? String)
        let pairing = try XCTUnwrap(bridge.pairingModel)
        await pairing.join(code)
        let peer = try await sasPeer(pairing)
        await pairing.confirm(requestId: peer.requestId, accept: true)
        try await offer.expectExit(0, timeout: 30)
        return bridge
    }
}
