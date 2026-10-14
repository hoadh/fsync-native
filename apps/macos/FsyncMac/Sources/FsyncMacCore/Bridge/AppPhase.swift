import FsyncCoreKit

public enum AppPhase: Equatable, Sendable {
    case booting
    case signedOut
    case needsPairing
    case ready
    case bootFailed(String, canReset: Bool)

    /// `Created` and `Ready` both mean this Mac is trusted; `NeedsPairing` means it must pair.
    public static func from(_ state: AccountState) -> AppPhase {
        switch state {
        case .created, .ready: return .ready
        case .needsPairing: return .needsPairing
        }
    }
}
