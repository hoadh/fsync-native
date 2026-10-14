import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

final class KeychainStoreTests: EngineTestCase {
    func testRoundTripOverwriteAndDelete() throws {
        let store = makeStore()
        XCTAssertNil(try store.get("x"))
        try store.set(Data([1, 2, 3]), account: "x")
        XCTAssertEqual(try store.get("x"), Data([1, 2, 3]))
        try store.set(Data([9]), account: "x")
        XCTAssertEqual(try store.get("x"), Data([9]))
        try store.delete("x")
        XCTAssertNil(try store.get("x"))
        XCTAssertNoThrow(try store.delete("x"), "deleting a missing item is fine")
        try store.delete("x")
    }

    func testAddDoesNotReplaceAnExistingItem() throws {
        let store = makeStore()
        XCTAssertTrue(try store.add(Data([1]), account: "a"))
        XCTAssertFalse(try store.add(Data([2]), account: "a"))
        XCTAssertEqual(try store.get("a"), Data([1]))
    }

    /// A probe item left by a crashed run (`errSecDuplicateItem`) still proves the store works.
    func testProbeTreatsDuplicateAsSupported() {
        XCTAssertTrue(KeychainStore.isSupported(addStatus: errSecSuccess))
        XCTAssertTrue(KeychainStore.isSupported(addStatus: errSecDuplicateItem))
        XCTAssertFalse(KeychainStore.isSupported(addStatus: errSecMissingEntitlement))
        XCTAssertFalse(KeychainStore.isSupported(addStatus: errSecNotAvailable))
    }

    private struct Boom: Error {}

    func testMigrationCopiesFromFileKeychainOnMiss() throws {
        var stored: Data?
        var dropped = false
        let got = try KeychainStore.readMigrating(
            primary: { stored }, legacy: { Data([7]) },
            store: { stored = $0 }, dropLegacy: { dropped = true })
        XCTAssertEqual(got, Data([7]))
        XCTAssertEqual(stored, Data([7]))
        XCTAssertTrue(dropped)
    }

    func testMigrationPrefersPrimaryAndLeavesLegacyAlone() throws {
        var touched = false
        let got = try KeychainStore.readMigrating(
            primary: { Data([1]) }, legacy: { touched = true; return Data([2]) },
            store: { _ in touched = true }, dropLegacy: { touched = true })
        XCTAssertEqual(got, Data([1]))
        XCTAssertFalse(touched)
    }

    func testMigrationMissEverywhereIsNil() throws {
        XCTAssertNil(try KeychainStore.readMigrating(
            primary: { nil }, legacy: { nil }, store: { _ in XCTFail("nothing to store") }, dropLegacy: {}))
    }

    /// If the copy fails the secret is still returned and the old item stays for the next try.
    func testMigrationKeepsLegacyWhenCopyFails() throws {
        var dropped = false
        let got = try KeychainStore.readMigrating(
            primary: { nil }, legacy: { Data([9]) },
            store: { _ in throw Boom() }, dropLegacy: { dropped = true })
        XCTAssertEqual(got, Data([9]))
        XCTAssertFalse(dropped)
    }

    func testMigrationReadErrorsPropagate() {
        XCTAssertThrowsError(try KeychainStore.readMigrating(
            primary: { throw KeychainError(status: -25308) }, legacy: { Data([1]) }, store: { _ in }, dropLegacy: {}))
    }

    /// The engine's session callback and the login model both save the session; neither may fail.
    func testConcurrentFirstWritesDoNotFail() async throws {
        for round in 0..<20 {
            let store = makeStore()
            let failures = await withTaskGroup(of: Bool.self) { group in
                for i in 0..<4 {
                    group.addTask { (try? store.set(Data([UInt8(i)]), account: "race\(round)")) == nil }
                }
                return await group.reduce(0) { $0 + ($1 ? 1 : 0) }
            }
            XCTAssertEqual(failures, 0, "round \(round)")
            try store.delete("race\(round)")
        }
    }

    func testServicesAreIsolated() throws {
        let a = makeStore(), b = makeStore()
        try a.set(Data([1]), account: AppSecrets.sessionAccount)
        XCTAssertNil(try b.get(AppSecrets.sessionAccount))
    }
}

final class AppSecretsTests: EngineTestCase {
    func testKeysAreStable32BytesAndIndependent() throws {
        let store = makeStore()
        let first = AppSecrets(store: store), second = AppSecrets(store: store)
        let secret = try first.loadOrCreateKey(AppSecrets.deviceSecretAccount)
        let history = try first.loadOrCreateKey(AppSecrets.historyKeyAccount)
        XCTAssertEqual(secret.count, 32)
        XCTAssertEqual(history.count, 32)
        XCTAssertNotEqual(secret, history)
        XCTAssertEqual(try second.loadOrCreateKey(AppSecrets.deviceSecretAccount), secret)
        XCTAssertEqual(try second.loadOrCreateKey(AppSecrets.historyKeyAccount), history)
    }

    func testWrongLengthKeyIsNotReplaced() throws {
        let store = makeStore()
        try store.set(Data([1, 2]), account: AppSecrets.historyKeyAccount)
        XCTAssertThrowsError(try AppSecrets(store: store).loadOrCreateKey(AppSecrets.historyKeyAccount)) {
            XCTAssertEqual($0 as? AppSecretsError, .corruptKey(AppSecrets.historyKeyAccount))
        }
        XCTAssertEqual(try store.get(AppSecrets.historyKeyAccount), Data([1, 2]))
    }

    func testLoadOrCreateKeyDoesNotOverwriteAConcurrentWinner() async throws {
        let store = makeStore()
        let secrets = AppSecrets(store: store)
        let keys = await withTaskGroup(of: Data?.self) { group in
            for _ in 0..<6 { group.addTask { try? secrets.loadOrCreateKey(AppSecrets.deviceSecretAccount) } }
            return await group.reduce(into: [Data?]()) { $0.append($1) }
        }
        XCTAssertEqual(Set(keys.compactMap { $0 }).count, 1, "every caller sees the one stored key")
        XCTAssertFalse(keys.contains { $0 == nil })
    }

    func testSessionSaveLoadClear() throws {
        let secrets = AppSecrets(store: makeStore())
        XCTAssertNil(try secrets.session())
        try secrets.saveSession(fakeTokens)
        XCTAssertEqual(try secrets.session(), fakeTokens)
        try secrets.clearSession()
        XCTAssertNil(try secrets.session())
    }

    func testUnreadableSessionCountsAsSignedOut() throws {
        let store = makeStore()
        try store.set(Data("not json".utf8), account: AppSecrets.sessionAccount)
        XCTAssertNil(try AppSecrets(store: store).session())
        XCTAssertNil(try store.get(AppSecrets.sessionAccount))
    }
}
