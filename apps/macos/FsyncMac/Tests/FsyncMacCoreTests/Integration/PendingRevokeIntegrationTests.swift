import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Hard logout after quit against the real local backend. Skipped unless `FSYNC_IT_API` is set.
@MainActor
final class PendingRevokeIntegrationTests: IntegrationTestCase {
    private func refreshStatus(_ token: String) async throws -> Int {
        var req = URLRequest(url: try XCTUnwrap(URL(string: backend.api + "/v1/auth/refresh")))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["refresh": token])
        let (_, response) = try await URLSession.shared.data(for: req)
        return (response as? HTTPURLResponse)?.statusCode ?? 0
    }

    /// The app "quit" before the revoke was confirmed: the next launch revokes from the Keychain entry.
    func testNextLaunchRevokesThePendingToken() async throws {
        let bridge = try await bootMac()
        try await signIn(bridge, email: uniqueEmail("pending"))
        let refresh = try XCTUnwrap(try bridge.secrets.session()).refresh
        try bridge.secrets.savePendingRevoke(PendingRevoke(refresh: refresh, apiBase: backend.api, createdAt: Date()))

        let secrets = bridge.secrets
        let outcome = await Task.detached { PendingRevokeResume.run(secrets: secrets) }.value
        XCTAssertEqual(outcome, .revoked)
        XCTAssertNil(try secrets.pendingRevoke(), "deleted after the revoke")
        let status = try await refreshStatus(refresh)
        XCTAssertEqual(status, 401, "the refresh token must be dead")

        let again = await Task.detached { PendingRevokeResume.run(secrets: secrets) }.value
        XCTAssertEqual(again, .nothingPending)
    }

    /// A normal sign-out with a reachable backend: revoked, and no pending entry is left behind.
    func testSignOutRevokesAndClearsPending() async throws {
        let bridge = try await bootMac()
        try await signIn(bridge, email: uniqueEmail("signout"))
        let refresh = try XCTUnwrap(try bridge.secrets.session()).refresh

        await bridge.signOut()
        XCTAssertEqual(bridge.phase, .signedOut)
        XCTAssertNil(try bridge.secrets.pendingRevoke(), "confirmed revoke clears the pending entry")
        XCTAssertNil(try bridge.secrets.session())
        let status = try await refreshStatus(refresh)
        XCTAssertEqual(status, 401)
    }
}
