import SwiftUI

/// Lists every dropped item before anything leaves this Mac. Cancel is the default button.
struct DropSendConfirmation: View {
    let request: DropRequest
    let onSend: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            let count = request.items.count
            Text(verbatim: "Send \(count == 1 ? "1 item" : "\(count) items") to \(request.deviceName)?").font(.title3)
            List(request.items, id: \.url.path) { item in
                HStack {
                    Image(systemName: item.isDirectory ? "folder" : "doc").foregroundStyle(.secondary).accessibilityHidden(true)
                    Text(verbatim: item.name).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 8)
                    Text(verbatim: item.sizeText).foregroundStyle(.secondary).monospacedDigit()
                }
            }
            .frame(height: min(CGFloat(count) * 28 + 12, 220))
            Text("\(request.deviceName) has to accept before the files arrive.").font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Send", action: onSend)
                // The default (Return) button is Cancel; sending takes a deliberate click.
                Button("Cancel", role: .cancel, action: onCancel).keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}
