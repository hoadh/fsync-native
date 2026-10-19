import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Files other devices want to send, running transfers and the last file error. "Transfers…" shows only while
/// there is an offer or a running transfer.
struct FilesMenuSection: View {
    @ObservedObject var files: FilesSync
    @ObservedObject var transfers: TransfersModel
    var openTransfers: (() -> Void)?

    var body: some View {
        let active = transfers.active
        if !files.offers.isEmpty || !active.isEmpty || files.message != nil {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(files.offers, id: \.itemId) { offer in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: "\(PeerText.sanitize(offer.fromName)) sends \(Self.summary(offer))").font(.caption).lineLimit(2)
                        HStack {
                            Button("Accept") { Task { await files.accept(offer) } }
                            Button("Decline") { files.decline(offer) }
                        }
                    }
                }
                ForEach(active) { row in
                    HStack {
                        ProgressView(value: row.fraction).frame(maxWidth: .infinity)
                        Text(verbatim: row.direction == .receive ? "Receiving" : "Sending").font(.caption).foregroundStyle(.secondary)
                        Button("Cancel") { Task { await transfers.cancel(row.id) } }.buttonStyle(.borderless).font(.caption)
                    }
                }
                if let message = files.message {
                    Text(message).font(.caption).foregroundStyle(.orange).onTapGesture { files.dismissMessage() }
                }
                if let openTransfers, !files.offers.isEmpty || !active.isEmpty {
                    Button("Transfers…", action: openTransfers).buttonStyle(MenuRowButtonStyle())
                }
            }
            Divider()
        }
    }

    static func summary(_ offer: ClipItem) -> String {
        guard let first = offer.files.first else { return "files" }
        let name = PeerText.sanitize(first.name, limit: 40)
        return offer.files.count == 1 ? name : "\(name) and \(offer.files.count - 1) more"
    }
}
