import FsyncCoreKit
import SwiftUI

/// Display helpers shared by the popover row and the Devices page row.
extension Device {
    var platformSymbol: String {
        switch platform {
        case .mac: return "laptopcomputer"
        case .win: return "desktopcomputer"
        case .cli: return "terminal"
        }
    }

    var platformName: String {
        switch platform {
        case .mac: return "Mac"
        case .win: return "Windows"
        case .cli: return "Command line"
        }
    }

    /// LAN / Relay / Online (known to the server, no connection yet) / Offline.
    var statusLabel: String {
        switch status {
        case .lan: return "LAN"
        case .relay: return "Relay"
        case .offline: return online ? "Online" : "Offline"
        }
    }

    /// Spec icon map: green = connected, orange = online but not connected, hollow = offline.
    var statusSymbol: String { status == .offline && !online ? "circle" : "circle.fill" }

    var statusColor: Color {
        switch status {
        case .lan, .relay: return .green
        case .offline: return online ? .orange : .secondary
        }
    }

    /// "Last seen 5 minutes ago" for an offline device that has been seen; nil otherwise.
    var lastSeenText: String? {
        guard !isSelf, status == .offline, !online, lastSeenMs > 0 else { return nil }
        let date = Date(timeIntervalSince1970: Double(lastSeenMs) / 1000)
        return "Last seen " + date.formatted(.relative(presentation: .named))
    }
}
