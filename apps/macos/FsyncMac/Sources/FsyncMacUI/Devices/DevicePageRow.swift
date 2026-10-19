import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// One row of the Devices page. The name comes from another device, so it is sanitized and verbatim.
struct DevicePageRow: View {
    let device: Device
    let isDropTargeted: Bool
    let onSend: () -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: device.platformSymbol).font(.title3).frame(width: 28)
                .foregroundStyle(.secondary).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: name).font(.body).lineLimit(1).truncationMode(.tail)
                Text(verbatim: device.isSelf ? "This Mac" : device.platformName)
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if !device.isSelf {
                VStack(alignment: .trailing, spacing: 2) {
                    HStack(spacing: 6) {
                        Image(systemName: device.statusSymbol).font(.system(size: 8))
                            .foregroundStyle(device.statusColor).accessibilityHidden(true)
                        Text(device.statusLabel).font(.callout)
                    }
                    if let seen = device.lastSeenText { Text(seen).font(.caption).foregroundStyle(.secondary) }
                }
                actionsMenu
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 6)
        .background(isDropTargeted ? Color.accentColor.opacity(0.15) : .clear, in: RoundedRectangle(cornerRadius: 8))
        .contentShape(Rectangle())
        .accessibilityElement(children: .contain)
        .contextMenu { if !device.isSelf { actions } }
    }

    private var name: String {
        DevicesPageLogic.displayName(device)
    }

    private var actionsMenu: some View {
        Menu { actions } label: { Image(systemName: "ellipsis.circle") }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .accessibilityLabel("Actions for \(name)")
    }

    @ViewBuilder private var actions: some View {
        Button("Send files…", action: onSend).disabled(!device.online)
        Divider()
        Button("Remove…", role: .destructive, action: onRemove)
    }
}
