import FsyncCoreKit
import Foundation
import os

/// Retries a pending revoke at launch with `revokeSession` (no engine needed).
public enum PendingRevokeResume {
    public enum Outcome: Equatable, Sendable {
        case nothingPending
        /// Revoked, or already revoked (401): deleted.
        case revoked
        /// Backend unreachable: kept with the failure counted.
        case kept(failures: Int)
        /// Final refusal, too many failures or too old: deleted without a revoke.
        case dropped
    }

    private static let log = Logger(subsystem: AppConfig.bundleId, category: "pending-revoke")

    /// Blocking (up to ~10 s): call it off the main thread. Never throws; Keychain trouble is logged.
    /// `revoke` is the real FFI call; only tests substitute it.
    public static func run(
        secrets: AppSecrets, now: Date = Date(),
        revoke: (String, String, Bool) throws -> Void = { try revokeSession(apiUrl: $0, refreshToken: $1, allowInsecureHttp: $2) }
    ) -> Outcome {
        let pending: PendingRevoke
        do {
            guard let found = try secrets.pendingRevoke() else { return .nothingPending }
            pending = found
        } catch {
            log.error("pending revoke unreadable: \(String(describing: error), privacy: .private)")
            return .nothingPending
        }
        if pending.isExpired(now: now) {
            clear(secrets)
            return .dropped
        }
        let insecure = AppConfig(apiBase: pending.apiBase, relayUrl: nil, deviceName: "").allowInsecureHttp
        do {
            try revoke(pending.apiBase, pending.refresh, insecure)
            clear(secrets)
            return .revoked
        } catch FsyncError.Network {
            var next = pending
            next.failures += 1
            if next.isExpired(now: now) {
                clear(secrets)
                return .dropped
            }
            do { try secrets.savePendingRevoke(next) } catch {
                log.error("could not record the failed revoke: \(String(describing: error), privacy: .private)")
            }
            return .kept(failures: next.failures)
        } catch {
            // A refusal that a retry will not change (bad URL, malformed token).
            log.error("pending revoke dropped: \(String(describing: error), privacy: .private)")
            clear(secrets)
            return .dropped
        }
    }

    private static func clear(_ secrets: AppSecrets) {
        do { try secrets.clearPendingRevoke() } catch {
            log.error("could not clear pending revoke: \(String(describing: error), privacy: .private)")
        }
    }
}
