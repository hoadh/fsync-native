import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// One device. The name comes from another device, so it is sanitized and shown verbatim.
struct DeviceRowView: View {
    let device: Device
    /// nil hides the remove action (the popover is read-only; this Mac can't be removed).
    let onRemove: (() -> Void)?

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon).frame(width: 20).foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: PeerText.sanitize(device.name)).lineLimit(1).truncationMode(.tail)
                Text(verbatim: detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            Circle().fill(color).frame(width: 8, height: 8).accessibilityHidden(true)
            Text(label).font(.caption).foregroundStyle(.secondary)
            if let onRemove, !device.isSelf {
                Button("Remove", role: .destructive, action: onRemove).buttonStyle(.borderless).font(.caption)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var icon: String {
        switch device.platform {
        case .mac: return "laptopcomputer"
        case .win: return "desktopcomputer"
        case .cli: return "terminal"
        }
    }

    private var label: String {
        switch device.status {
        case .lan: return "LAN"
        case .relay: return "Relay"
        case .offline: return device.online ? "Online" : "Offline"
        }
    }

    private var color: Color {
        switch device.status {
        case .lan: return .green
        case .relay: return .orange
        case .offline: return device.online ? .blue : .gray
        }
    }

    private var detail: String {
        if device.isSelf { return "This Mac" }
        guard device.status == .offline, !device.online, device.lastSeenMs > 0 else { return "" }
        let date = Date(timeIntervalSince1970: Double(device.lastSeenMs) / 1000)
        return "Last seen " + date.formatted(.relative(presentation: .named))
    }
}
