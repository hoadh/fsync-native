import AppKit

/// What macOS says about this app reading other apps' clipboard content (macOS 15.4+ privacy control).
public enum PasteboardAccess: Equatable, Sendable {
    /// Older macOS, or access is allowed: nothing to explain.
    case allowed
    /// macOS asks the user the first time content is read; auto mode may show an alert.
    case asks
    case denied

    @MainActor
    public static func current(_ pasteboard: NSPasteboard = .general) -> PasteboardAccess {
        guard #available(macOS 15.4, *) else { return .allowed }
        switch pasteboard.accessBehavior {
        case .alwaysAllow: return .allowed
        case .alwaysDeny: return .denied
        default: return .asks
        }
    }

    public var explanation: String? {
        switch self {
        case .allowed: return nil
        case .asks:
            return "macOS may ask before fsync reads what you copy. Choose Allow. If you'd rather not, switch to manual mode and send with the shortcut."
        case .denied:
            return "fsync is not allowed to read the clipboard. Allow it under System Settings > Privacy & Security > Paste from Other Apps."
        }
    }
}
