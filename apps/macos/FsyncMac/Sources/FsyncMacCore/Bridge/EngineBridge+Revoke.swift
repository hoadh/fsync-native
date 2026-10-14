import FsyncCoreKit
import Foundation

/// Hard logout after quit: the refresh token is kept in the Keychain as a "pending revoke" until
/// the backend confirms the revoke (docs/ffi-contract.md, "Hard logout after quit").
extension EngineBridge {
    /// Sign-out half that talks to the backend. Order matters: persist the pending revoke BEFORE
    /// `logoutConfirmed()`, so a quit during the call still leaves a retry for the next launch.
    func logoutKeepingRevoke() async {
        let (secrets, apiBase, engine) = (secrets, config.apiBase, engine)
        await Task.detached {
            let saved = Self.savePendingRevoke(secrets: secrets, apiBase: apiBase)
            guard let engine else {
                // No engine to log out with (boot failed): revoke directly.
                if saved != nil { _ = PendingRevokeResume.run(secrets: secrets) }
                return
            }
            do {
                // Only drop the entry this sign-out saved; a newer one stays for the next launch.
                if try engine.logoutConfirmed(), let saved {
                    try? secrets.settlePendingRevoke(retried: saved, replacement: nil)
                }
            } catch {
                Self.log.error("logout failed: \(String(describing: error), privacy: .private)")
            }
        }.value
    }

    /// Retries a revoke left by an earlier sign-out. Off the main thread, never blocks boot.
    func resumePendingRevoke() {
        let secrets = secrets
        Task.detached(priority: .utility) {
            let outcome = PendingRevokeResume.run(secrets: secrets)
            if outcome != .nothingPending { Self.log.info("pending revoke: \(String(describing: outcome))") }
        }
    }

    /// The newest refresh token the Keychain holds (every rotation is stored by the delegate).
    private nonisolated static func savePendingRevoke(secrets: AppSecrets, apiBase: String) -> PendingRevoke? {
        do {
            guard let session = try secrets.session() else { return nil }
            let pending = PendingRevoke(refresh: session.refresh, apiBase: apiBase, createdAt: Date())
            try secrets.savePendingRevoke(pending)
            return pending
        } catch {
            Self.log.error("could not keep a pending revoke: \(String(describing: error), privacy: .private)")
            return nil
        }
    }
}
