import FsyncCoreKit
import FsyncMacCore
import SwiftUI

struct HistoryRowView: View {
    let item: HistoryItem
    /// Resolved by the caller from the device list (see `PeerText.deviceName`).
    let peerName: String
    let onCopy: (() -> Void)?
    let onPin: () -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: Self.symbol(item.kind)).frame(width: 20).foregroundStyle(.secondary).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: item.preview.map { PeerText.sanitize($0, limit: 200) } ?? Self.fallback(item.kind)).lineLimit(2)
                Text(verbatim: caption).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if let onCopy { iconButton("Copy", "doc.on.doc", onCopy) }
            iconButton(item.pinned ? "Unpin" : "Pin", item.pinned ? "pin.fill" : "pin", onPin)
            iconButton("Delete", "trash", onDelete, role: .destructive)
        }
        .buttonStyle(.borderless)
        .padding(.vertical, 4)
        .contextMenu {
            if let onCopy { Button("Copy", action: onCopy) }
            Button(item.pinned ? "Unpin" : "Pin", action: onPin)
            Button("Delete", role: .destructive, action: onDelete)
        }
    }

    /// Icon-only action; the label is the VoiceOver name and the tooltip.
    private func iconButton(_ label: String, _ symbol: String, _ action: @escaping () -> Void,
                            role: ButtonRole? = nil) -> some View {
        Button(role: role, action: action) { Image(systemName: symbol) }
            .help(label).accessibilityLabel(label)
    }

    private var caption: String {
        let who = item.outgoing ? "Sent" : "From \(peerName)"
        let when = Date(timeIntervalSince1970: Double(item.createdMs) / 1000).formatted(.relative(presentation: .named))
        let size = ByteCountFormatter.string(fromByteCount: Int64(item.totalSize), countStyle: .file)
        return "\(who) · \(when) · \(size)"
    }

    static func symbol(_ kind: ClipKind) -> String {
        switch kind {
        case .text: return "text.alignleft"
        case .rich: return "textformat"
        case .image: return "photo"
        case .files: return "doc"
        }
    }

    static func fallback(_ kind: ClipKind) -> String {
        switch kind {
        case .text, .rich: return "Clip"
        case .image: return "Image"
        case .files: return "Files"
        }
    }
}
