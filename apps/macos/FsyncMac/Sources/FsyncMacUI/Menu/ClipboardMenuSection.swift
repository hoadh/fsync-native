import FsyncMacCore
import SwiftUI

/// Clipboard actions in the popover: send now (manual mode), paste what another device sent, last activity.
struct ClipboardMenuSection: View {
    @ObservedObject var sync: ClipboardSync
    let openHistory: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let item = sync.pendingIncoming {
                Button { Task { await sync.acceptPending() } } label: {
                    Text(verbatim: "Paste from \(PeerText.sanitize(item.fromName))")
                }
                if let preview = item.preview { Text(verbatim: PeerText.sanitize(preview, limit: 120)).font(.caption).lineLimit(2).foregroundStyle(.secondary) }
            }
            Button { Task { await sync.publishCurrent() } } label: {
                Text(verbatim: "Send clipboard  \(HotkeyManager.sendClipboardLabel)")
            }
            .disabled(!sync.isRunning)
            Button("History…", action: openHistory)
            if let activity = sync.activity {
                Text(verbatim: "\(activity.direction == .sent ? "Sent" : "Received"): \(PeerText.sanitize(activity.summary, limit: 80))")
                    .font(.caption).lineLimit(1).foregroundStyle(.secondary)
            }
            if let message = sync.message {
                Text(message).font(.caption).foregroundStyle(.orange).onTapGesture { sync.dismissMessage() }
            }
        }
    }
}
