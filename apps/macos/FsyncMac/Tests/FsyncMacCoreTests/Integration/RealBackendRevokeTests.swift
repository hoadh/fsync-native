import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// The pending-revoke retry over real TLS against a deployed backend. Skipped unless `FSYNC_REAL_API`
/// is set (for example `https://api-fsync.10z.one`), so a plain `swift test` never touches the network.
///
/// A real session needs an email OTP (the deployed backend sends it by SMTP, there is no log path), so
/// by default the token is an unknown one: the server answers 204 for it (revoke is idempotent), which
/// is the same answer a revoked token gets. Set `FSYNC_REAL_REFRESH` to a live refresh token (from your
/// own sign-in) to also prove the session dies server-side. The token is never printed.
final class RealBackendRevokeTests: EngineTestCase {
    private var api: String {
        get throws {
            guard let api = ProcessInfo.processInfo.environment["FSYNC_REAL_API"], !api.isEmpty else {
                throw XCTSkip("FSYNC_REAL_API is not set")
            }
            return api
        }
    }

    private func refreshStatus(api: String, token: String) async throws -> Int {
        var req = URLRequest(url: try XCTUnwrap(URL(string: api + "/v1/auth/refresh")), timeoutInterval: 20)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["refresh": token])
        let (_, response) = try await URLSession.shared.data(for: req)
        return (response as? HTTPURLResponse)?.statusCode ?? 0
    }

    private func retry(_ secrets: AppSecrets) async -> PendingRevokeResume.Outcome {
        await Task.detached { PendingRevokeResume.run(secrets: secrets) }.value
    }

    /// An unknown token: the real backend settles it, the Keychain entry is deleted, a second launch is a no-op.
    func testPendingRevokeIsSettledByTheDeployedBackend() async throws {
        let api = try api
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(PendingRevoke(refresh: "unknown-\(UUID().uuidString)", apiBase: api, createdAt: Date()))
        let outcome = await retry(secrets)
        XCTAssertEqual(outcome, .revoked)
        XCTAssertNil(try secrets.pendingRevoke())
        let again = await retry(secrets)
        XCTAssertEqual(again, .nothingPending)
    }

    /// Backend unreachable (right host, closed port): kept and counted, the token is still there for the next launch.
    func testUnreachableDeployedHostIsKeptThenDropped() async throws {
        let api = try api
        let host = try XCTUnwrap(URL(string: api)?.host)
        let secrets = AppSecrets(store: makeStore())
        let dead = "https://\(host):9"
        try secrets.savePendingRevoke(PendingRevoke(refresh: "unknown-token", apiBase: dead, createdAt: Date(), failures: PendingRevoke.maxFailures - 2))
        let first = await retry(secrets)
        XCTAssertEqual(first, .kept(failures: PendingRevoke.maxFailures - 1))
        let last = await retry(secrets)
        XCTAssertEqual(last, .dropped, "the fifth failed launch gives up")
        XCTAssertNil(try secrets.pendingRevoke())
    }

    /// Owner-supplied live token: after the retry the server must refuse it.
    func testLiveRefreshTokenIsRevokedServerSide() async throws {
        let api = try api
        guard let token = ProcessInfo.processInfo.environment["FSYNC_REAL_REFRESH"], !token.isEmpty else {
            throw XCTSkip("FSYNC_REAL_REFRESH is not set")
        }
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(PendingRevoke(refresh: token, apiBase: api, createdAt: Date()))
        let outcome = await retry(secrets)
        XCTAssertEqual(outcome, .revoked)
        let status = try await refreshStatus(api: api, token: token)
        XCTAssertEqual(status, 401, "the refresh token must be dead")
    }
}
