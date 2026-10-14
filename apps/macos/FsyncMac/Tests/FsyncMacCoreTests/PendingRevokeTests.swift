import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Real Keychain (throwaway service) and the real `revokeSession` against a closed port.
final class PendingRevokeTests: EngineTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private func pending(failures: Int = 0, age: TimeInterval = 60, api: String = "http://127.0.0.1:9") -> PendingRevoke {
        PendingRevoke(refresh: "refresh-token", apiBase: api, createdAt: now.addingTimeInterval(-age), failures: failures)
    }

    func testKeychainRoundTripAndClear() throws {
        let secrets = AppSecrets(store: makeStore())
        XCTAssertNil(try secrets.pendingRevoke())
        try secrets.savePendingRevoke(pending(failures: 2))
        XCTAssertEqual(try secrets.pendingRevoke(), pending(failures: 2))
        try secrets.clearPendingRevoke()
        XCTAssertNil(try secrets.pendingRevoke())
    }

    func testUnreadableEntryIsDropped() throws {
        let store = makeStore()
        try store.set(Data("not json".utf8), account: AppSecrets.pendingRevokeAccount)
        XCTAssertNil(try AppSecrets(store: store).pendingRevoke())
        XCTAssertNil(try store.get(AppSecrets.pendingRevokeAccount))
    }

    func testExpiryByFailuresAndAge() {
        XCTAssertFalse(pending().isExpired(now: now))
        XCTAssertFalse(pending(failures: PendingRevoke.maxFailures - 1).isExpired(now: now))
        XCTAssertTrue(pending(failures: PendingRevoke.maxFailures).isExpired(now: now))
        XCTAssertFalse(pending(age: PendingRevoke.maxAge - 1).isExpired(now: now))
        XCTAssertTrue(pending(age: PendingRevoke.maxAge).isExpired(now: now))
    }

    func testDescriptionHidesTheToken() {
        XCTAssertFalse("\(pending())".contains("refresh-token"))
    }

    func testNothingPending() {
        let outcome = PendingRevokeResume.run(secrets: AppSecrets(store: makeStore()), now: now)
        XCTAssertEqual(outcome, .nothingPending)
    }

    func testUnreachableBackendKeepsAndCountsFailures() throws {
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(pending())
        XCTAssertEqual(PendingRevokeResume.run(secrets: secrets, now: now), .kept(failures: 1))
        XCTAssertEqual(PendingRevokeResume.run(secrets: secrets, now: now), .kept(failures: 2))
        XCTAssertEqual(try secrets.pendingRevoke()?.failures, 2)
        XCTAssertEqual(try secrets.pendingRevoke()?.refresh, "refresh-token")
    }

    func testDroppedAfterTheLastFailure() throws {
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(pending(failures: PendingRevoke.maxFailures - 1))
        XCTAssertEqual(PendingRevokeResume.run(secrets: secrets, now: now), .dropped)
        XCTAssertNil(try secrets.pendingRevoke())
    }

    func testDroppedWhenOlderThanThirtyDaysWithoutCalling() throws {
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(pending(age: PendingRevoke.maxAge + 1))
        var called = false
        let outcome = PendingRevokeResume.run(secrets: secrets, now: now) { _, _, _ in called = true }
        XCTAssertEqual(outcome, .dropped)
        XCTAssertFalse(called)
        XCTAssertNil(try secrets.pendingRevoke())
    }

    func testInvalidInputIsDroppedByTheRealCall() throws {
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(pending(api: "not a url"))
        XCTAssertEqual(PendingRevokeResume.run(secrets: secrets, now: now), .dropped)
        XCTAssertNil(try secrets.pendingRevoke())
    }

    func testSuccessDeletesAndPassesTheStoredValues() throws {
        let secrets = AppSecrets(store: makeStore())
        try secrets.savePendingRevoke(pending(api: "https://api.example.test"))
        var seen: (String, String, Bool)?
        let outcome = PendingRevokeResume.run(secrets: secrets, now: now) { seen = ($0, $1, $2) }
        XCTAssertEqual(outcome, .revoked)
        XCTAssertEqual(seen?.0, "https://api.example.test")
        XCTAssertEqual(seen?.1, "refresh-token")
        XCTAssertEqual(seen?.2, false)
        XCTAssertNil(try secrets.pendingRevoke())
    }
}
