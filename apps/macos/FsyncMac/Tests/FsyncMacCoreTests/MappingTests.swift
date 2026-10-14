import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

final class AppPhaseMappingTests: XCTestCase {
    func testAccountStateToPhase() {
        XCTAssertEqual(AppPhase.from(.created), .ready)
        XCTAssertEqual(AppPhase.from(.ready), .ready)
        XCTAssertEqual(AppPhase.from(.needsPairing), .needsPairing)
    }
}

final class AppConfigTests: XCTestCase {
    func testEnvOverridesInfoAndEmptyIsUnset() {
        let info: [String: Any] = ["FsyncApiBase": "https://api.example.com", "FsyncRelayUrl": "https://relay.example.com"]
        let c = AppConfig.resolve(env: ["FSYNC_API": "http://localhost:1", "FSYNC_RELAY": " "], info: info, deviceName: "M")
        XCTAssertEqual(c.apiBase, "http://localhost:1")
        XCTAssertEqual(c.relayUrl, "https://relay.example.com")
        XCTAssertEqual(AppConfig.resolve(env: [:], info: nil, deviceName: "M").apiBase, AppConfig.defaultApiBase)
    }

    func testRelayUrlListSplitsOnCommas() {
        func urls(_ raw: String?) -> [String] { AppConfig(apiBase: "https://a", relayUrl: raw, deviceName: "M").relayUrls }
        XCTAssertEqual(urls(nil), [])
        XCTAssertEqual(urls("https://r1"), ["https://r1"])
        XCTAssertEqual(urls(" https://r1 , https://r2,, "), ["https://r1", "https://r2"])
    }

    func testInsecureHttpOnlyForLocalhost() {
        func allow(_ url: String) -> Bool { AppConfig(apiBase: url, relayUrl: nil, deviceName: "M").allowInsecureHttp }
        XCTAssertTrue(allow("http://127.0.0.1:8080"))
        XCTAssertTrue(allow("http://localhost:8080"))
        XCTAssertFalse(allow("http://example.com"))
        XCTAssertFalse(allow("http://127.0.0.1.evil.com"))
        XCTAssertFalse(allow("https://127.0.0.1"))
    }
}

final class ErrorTextTests: XCTestCase {
    func testMapping() {
        func text(_ e: FsyncError, _ c: ErrorContext = .general) -> String? { ErrorText.message(for: e, context: c) }
        XCTAssertEqual(text(.Network(msg: "x")), "Can't reach fsync. Check your connection.")
        XCTAssertEqual(text(.Auth), "Sign in again.")
        XCTAssertEqual(text(.ReauthRequired), "Sign in again.")
        XCTAssertEqual(text(.Auth, .login), "Code is wrong or expired")
        XCTAssertEqual(text(.Invalid(msg: "x"), .login), "Code is wrong or expired")
        XCTAssertEqual(text(.Invalid(msg: "x"), .pairing), "Code invalid or expired")
        XCTAssertEqual(text(.NotFound), "This changed. Review it again.")
        XCTAssertEqual(text(.PeerOffline), "That device is offline.")
        XCTAssertNil(text(.Cancelled))
        XCTAssertEqual(text(.Io(msg: "secret path")), "Something went wrong (io)")
        XCTAssertEqual(text(.Internal(msg: "x")), "Something went wrong (internal)")
        XCTAssertEqual(text(.NotPaired), "Something went wrong (not-paired)")
        XCTAssertNotNil(ErrorText.message(for: CancellationError(), context: .general))
    }

    func testRequiresSignIn() {
        XCTAssertTrue(ErrorText.requiresSignIn(FsyncError.Auth))
        XCTAssertTrue(ErrorText.requiresSignIn(FsyncError.ReauthRequired))
        XCTAssertFalse(ErrorText.requiresSignIn(FsyncError.Network(msg: "x")))
    }
}
