import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// "Waiting for you": files other devices want to send. Nothing is saved until the user accepts.
struct TransferOfferSection: View {
    @ObservedObject var files: FilesSync

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Waiting for you").font(.headline)
            ForEach(files.offers, id: \.itemId) { offer in
                HStack(spacing: 10) {
                    Image(systemName: "tray.and.arrow.down").foregroundStyle(.secondary).accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: "\(Self.sender(offer)) · \(Self.countText(offer))").lineLimit(1)
                        Text(verbatim: "\(FilesMenuSection.summary(offer)) · \(Self.sizeText(offer))")
                            .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer(minLength: 8)
                    Button("Decline") { files.decline(offer) }
                    Button("Save to Downloads") { Task { await files.accept(offer) } }
                }
                .accessibilityElement(children: .contain)
            }
        }
        .padding(12)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
    }

    static func sender(_ offer: ClipItem) -> String {
        let name = PeerText.sanitize(offer.fromName)
        return name.isEmpty ? "another device" : name
    }

    static func countText(_ offer: ClipItem) -> String {
        offer.files.count == 1 ? "1 file" : "\(offer.files.count) files"
    }

    static func sizeText(_ offer: ClipItem) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: offer.totalSize), countStyle: .file)
    }
}
