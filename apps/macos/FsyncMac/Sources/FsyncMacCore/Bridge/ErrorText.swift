import FsyncCoreKit
import os

public enum ErrorContext: Sendable {
    case login, pairing, general
}

/// One mapping from engine errors to short user text. Details never reach the UI.
public enum ErrorText {
    private static let log = Logger(subsystem: AppConfig.bundleId, category: "engine-error")

    public static let signInAgain = "Sign in again."

    /// Text to show, or nil when the error should be silent (`Cancelled`).
    public static func message(for error: Error, context: ErrorContext) -> String? {
        guard let e = error as? FsyncError else { return generic("unknown", error) }
        switch e {
        case .Network: return "Can't reach fsync. Check your connection."
        case .Auth, .ReauthRequired:
            // During sign-in a rejected code can come back as Auth.
            return context == .login ? invalidText(context) : signInAgain
        case .Invalid: return invalidText(context)
        case .NotFound: return "This changed. Review it again."
        case .PeerOffline: return "That device is offline."
        case .HistoryKeyMismatch: return "The saved history belongs to a different key."
        case .Cancelled: return nil
        case .Io: return generic("io", e)
        case .Integrity: return generic("integrity", e)
        case .TooLarge: return "Too large to sync. Raise the limit in Settings or copy less."
        case .NotPaired: return generic("not-paired", e)
        case .Internal: return generic("internal", e)
        }
    }

    /// True when the user must sign in again.
    public static func requiresSignIn(_ error: Error) -> Bool {
        switch error as? FsyncError {
        case .Auth?, .ReauthRequired?: return true
        default: return false
        }
    }

    private static func invalidText(_ context: ErrorContext) -> String {
        switch context {
        case .login: return "Code is wrong or expired"
        case .pairing: return "Code invalid or expired"
        case .general: return "That didn't work. Check it and try again."
        }
    }

    private static func generic(_ code: String, _ error: Error) -> String {
        log.error("engine error \(code, privacy: .public): \(String(describing: error), privacy: .private)")
        return "Something went wrong (\(code))"
    }
}
