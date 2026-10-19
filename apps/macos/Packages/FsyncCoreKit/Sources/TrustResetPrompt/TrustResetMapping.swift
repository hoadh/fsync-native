import FsyncCoreKit

/// Pure mappings from engine results to what the prompt shows.
enum TrustResetMapping {
    /// Text for the prompt; `isNotice` marks informational text (not a failure).
    struct Message: Equatable {
        let text: String
        let isNotice: Bool
    }

    static let listChanged = Message(text: "The device list changed. Review it again.", isNotice: true)

    /// Result of `confirmTrustReset`. `created` is not expected here and counts as trusted.
    static func outcome(for state: AccountState) -> TrustResetOutcome {
        switch state {
        case .ready, .created: return .trusted
        case .needsPairing: return .removedNeedsPairing
        }
    }

    /// Short user-readable text for an engine error. Details are never shown.
    static func message(for error: Error) -> Message {
        switch error {
        case FsyncError.NotFound:
            return Message(text: "This request changed or was already answered. Review the latest one.", isNotice: true)
        case FsyncError.Network:
            return Message(text: "Couldn't reach the server. Check your connection and try again.", isNotice: false)
        case FsyncError.Auth, FsyncError.ReauthRequired:
            return Message(text: "Please sign in again, then retry.", isNotice: false)
        default:
            return Message(text: "Something went wrong. Please try again.", isNotice: false)
        }
    }

    /// The confirmation dialog may only show while it was opened for the reset on screen.
    static func dialogIsPresented(confirmingId: String?, displayedId: String?) -> Bool {
        confirmingId != nil && confirmingId == displayedId
    }

    /// Spec section 5 sanitizer (same algorithm as `PeerText.sanitize`, which lives in FsyncMacCore and
    /// cannot be imported here): drops control and format scalars, trims, caps at 40 characters.
    static func sanitize(_ text: String, limit: Int = 40) -> String {
        let cleaned = String(String.UnicodeScalarView(text.unicodeScalars.filter {
            $0.properties.generalCategory != .control && $0.properties.generalCategory != .format
        })).trimmingCharacters(in: .whitespaces)
        guard cleaned.count > limit else { return cleaned }
        return String(cleaned.prefix(limit)) + "…"
    }

    /// Names of the new device list as shown: sanitized, empty ones replaced by a neutral label.
    static func displayNames(_ names: [String]) -> [String] {
        names.map { name in
            let clean = sanitize(name)
            return clean.isEmpty ? "another device" : clean
        }
    }
}
