import FsyncCoreKit
import Foundation
import Security

public enum AppSecretsError: Error, Equatable {
    case randomFailed(OSStatus)
    /// A stored key is not 32 bytes. Not replaced silently: that would orphan the history.
    case corruptKey(String)
}

/// The app's secrets in the Keychain: device secret, history key, session tokens.
public struct AppSecrets: Sendable {
    public static let deviceSecretAccount = "device_secret"
    public static let historyKeyAccount = "history_key"
    public static let sessionAccount = "session"
    public static let keyLength = 32

    private let store: KeychainStore

    public init(store: KeychainStore = KeychainStore()) {
        self.store = store
    }

    /// The 32-byte key stored under `account`, created from `SecRandomCopyBytes` on first use.
    public func loadOrCreateKey(_ account: String) throws -> Data {
        if let existing = try store.get(account) {
            guard existing.count == Self.keyLength else { throw AppSecretsError.corruptKey(account) }
            return existing
        }
        var bytes = [UInt8](repeating: 0, count: Self.keyLength)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        guard status == errSecSuccess else { throw AppSecretsError.randomFailed(status) }
        let key = Data(bytes)
        // Add-only: if another writer got there first, theirs wins and is read back.
        if try store.add(key, account: account) { return key }
        guard let existing = try store.get(account), existing.count == Self.keyLength else {
            throw AppSecretsError.corruptKey(account)
        }
        return existing
    }

    private struct StoredSession: Codable {
        let access: String
        let refresh: String
        let accountId: String
    }

    /// Stored tokens, or nil. Unreadable JSON counts as no session (the user signs in again).
    public func session() throws -> SessionTokens? {
        guard let data = try store.get(Self.sessionAccount) else { return nil }
        guard let s = try? JSONDecoder().decode(StoredSession.self, from: data) else {
            try store.delete(Self.sessionAccount)
            return nil
        }
        return SessionTokens(access: s.access, refresh: s.refresh, accountId: s.accountId)
    }

    public func saveSession(_ tokens: SessionTokens) throws {
        let data = try JSONEncoder().encode(
            StoredSession(access: tokens.access, refresh: tokens.refresh, accountId: tokens.accountId))
        try store.set(data, account: Self.sessionAccount)
    }

    public func clearSession() throws {
        try store.delete(Self.sessionAccount)
    }
}
