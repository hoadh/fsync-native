import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Both sides must see the same words. "They don't match" is the default button; there is no auto-accept.
struct SasConfirmView: View {
    let peer: PairingPeer
    /// Called with the request id that is on screen and the user's answer.
    let onAnswer: (UInt64, Bool) -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text("Check the words").font(.title2)
            Text("The other device must show exactly these words.").foregroundStyle(.secondary)
            Text(verbatim: PeerText.sanitize(peer.sasWords, limit: 80))
                .font(.system(size: 26, weight: .bold, design: .rounded))
                .multilineTextAlignment(.center).padding(.vertical, 6)
            VStack(spacing: 2) {
                Text(verbatim: PeerText.sanitize(peer.name)).font(.headline)
                Text(verbatim: "\(platformName) · \(peer.nodeId.prefix(8))").font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("They match") { onAnswer(peer.requestId, true) }
                Button("They don't match") { onAnswer(peer.requestId, false) }
                    .keyboardShortcut(.defaultAction)
            }
            Text("Only pair if the words are identical and you started this yourself.")
                .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .id(peer.requestId)  // a newer request starts a fresh view
    }

    private var platformName: String {
        switch peer.platform {
        case .mac: return "Mac"
        case .win: return "Windows"
        case .cli: return "Command line"
        }
    }
}
