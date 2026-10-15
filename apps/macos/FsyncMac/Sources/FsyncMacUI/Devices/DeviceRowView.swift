import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// One device. The name comes from another device, so it is sanitized and shown verbatim.
struct DeviceRowView: View {
    let device: Device
    /// nil hides the remove action (the popover is read-only; this Mac can't be removed).
    let onRemove: (() -> Void)?
    /// Files chosen with "Send files…" or dropped on the row; nil hides both.
    var onSend: (([URL]) -> Void)?

    @State private var dropTargeted = false

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon).frame(width: 20).foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: PeerText.sanitize(device.name)).lineLimit(1).truncationMode(.tail)
                Text(verbatim: detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 4)
            // The engine has no connection to itself, so this Mac's status is always "offline";
            // "This Mac" under the name says enough.
            if !device.isSelf {
                Circle().fill(color).frame(width: 8, height: 8).accessibilityHidden(true)
                Text(label).font(.caption).foregroundStyle(.secondary)
            }
            if let onSend, !device.isSelf, device.online {
                Button("Send files…") { if let urls = FilePicker.chooseFiles() { onSend(urls) } }
                    .buttonStyle(.borderless).font(.caption)
            }
            if let onRemove, !device.isSelf {
                Button("Remove", role: .destructive, action: onRemove).buttonStyle(.borderless).font(.caption)
            }
        }
        .accessibilityElement(children: .combine)
        .padding(2)
        .background(dropTargeted ? Color.accentColor.opacity(0.15) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .onDrop(of: [.fileURL], isTargeted: $dropTargeted) { providers in
            guard let onSend, !device.isSelf, device.online else { return false }
            FilePicker.fileURLs(from: providers, completion: onSend)
            return true
        }
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
