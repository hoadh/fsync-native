import Foundation

/// A refresh token whose server-side revocation is not confirmed yet (hard logout after quit).
///
/// Core never persists the refresh token, so `logout` may leave it valid for up to 30 days when
/// the app quits before the backend answered. The host keeps it here, in the Keychain, until a
/// revoke is confirmed. See "Hard logout after quit" in docs/ffi-contract.md.
public struct PendingRevoke: Codable, Equatable, Sendable, CustomStringConvertible {
    public let refresh: String
    /// The backend that issued the token; the revoke must go there even if the config changed.
    public let apiBase: String
    public let createdAt: Date
    public var failures: Int

    public init(refresh: String, apiBase: String, createdAt: Date, failures: Int = 0) {
        self.refresh = refresh
        self.apiBase = apiBase
        self.createdAt = createdAt
        self.failures = failures
    }

    /// Give up after this many launches that could not reach the backend.
    public static let maxFailures = 5
    /// The refresh token expires after 30 days anyway.
    public static let maxAge: TimeInterval = 30 * 24 * 3600

    /// Past retrying: too many failures, or old enough that the token has expired.
    public func isExpired(now: Date) -> Bool {
        failures >= Self.maxFailures || now.timeIntervalSince(createdAt) >= Self.maxAge
    }

    /// Never prints the token.
    public var description: String { "PendingRevoke(apiBase: \(apiBase), failures: \(failures))" }
}

extension AppSecrets {
    public static let pendingRevokeAccount = "pending_revoke"

    /// The pending revoke, or nil. Unreadable JSON is dropped (nothing can be done with it).
    public func pendingRevoke() throws -> PendingRevoke? {
        guard let data = try store.get(Self.pendingRevokeAccount) else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        guard let pending = try? decoder.decode(PendingRevoke.self, from: data) else {
            try store.delete(Self.pendingRevokeAccount)
            return nil
        }
        return pending
    }

    /// Persists (replaces) the pending revoke. Call BEFORE `logoutConfirmed()`.
    public func savePendingRevoke(_ pending: PendingRevoke) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        try store.set(try encoder.encode(pending), account: Self.pendingRevokeAccount)
    }

    public func clearPendingRevoke() throws {
        try store.delete(Self.pendingRevokeAccount)
    }
}
