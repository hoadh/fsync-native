import FsyncCoreKit
import SwiftUI

/// The question: warning, new device list, and the two answers. Owns the confirmation state, so
/// give it `.id(info.resetId)` to start fresh (dialog closed) whenever the shown reset changes.
struct TrustResetPendingView: View {
    let info: TrustResetInfo
    let isBusy: Bool
    let errorMessage: String?
    let errorIsNotice: Bool
    let onKeep: (String) -> Void
    let onTrust: (String) -> Void

    /// Id the confirmation dialog was opened for; answers use this, not whatever is current.
    @State private var confirmingId: String?

    private static let visibleRows = 6

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle.fill").font(.title).foregroundColor(.orange)
                    .accessibilityHidden(true)
                Text("Your trusted devices were reset").font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
            }
            Text("Someone signed in to your account and reset the trusted devices. Only accept if you did this yourself or know who did. Check that the devices below are the ones you expect. If you keep the current devices, this Mac continues to trust the devices it trusts now.")
                .fixedSize(horizontal: false, vertical: true)
            deviceList
            if let errorMessage {
                // Notices (request changed) are informational; real errors stay red.
                Text(verbatim: errorMessage).foregroundColor(errorIsNotice ? .noticeOrange : .red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityLabel(errorIsNotice ? errorMessage : "Error: \(errorMessage)")
            }
            buttons
        }
    }

    private var deviceList: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("New device list (\(info.devices))").font(.headline)
            // Core already rejects control/bidi characters and names over 64 chars
            // (fsync-proto check_shape), so names are not filtered again here.
            let rows = VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(info.deviceNames.enumerated()), id: \.offset) { _, name in
                    // Only names are known, not platforms, so the bullet is neutral.
                    Label {
                        // Verbatim: names are untrusted text, no markdown or links.
                        Text(verbatim: name).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image(systemName: "circle.fill").font(.system(size: 5)).foregroundColor(.secondary)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Group {
                if info.deviceNames.count > Self.visibleRows {
                    ScrollView { rows }.frame(maxHeight: 150)
                } else {
                    rows
                }
            }
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
        }
    }

    private var buttons: some View {
        HStack {
            if isBusy { ProgressView().controlSize(.small).accessibilityLabel("Working") }
            Spacer()
            Button("Trust new devices") { confirmingId = info.resetId }
                .disabled(isBusy)
                .accessibilityHint("Replaces the devices this Mac trusts")
            Button("Keep current devices") { onKeep(info.resetId) }
                .keyboardShortcut(.defaultAction)
                .disabled(isBusy)
        }
        .confirmationDialog(
            "Trust the new devices?",
            isPresented: Binding(
                get: { TrustResetMapping.dialogIsPresented(confirmingId: confirmingId, displayedId: info.resetId) },
                set: { if !$0 { confirmingId = nil } }),
            titleVisibility: .visible,
            presenting: confirmingId
        ) { id in
            // `id` is captured when the dialog opens, so closing it first cannot drop the answer.
            Button("Trust new devices", role: .destructive) { onTrust(id) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(verbatim: dialogMessage)
        }
    }

    /// Lists what will be trusted so the second step shows it, not just a warning.
    private var dialogMessage: String {
        let shown = info.deviceNames.prefix(Self.visibleRows)
        let more = info.deviceNames.count - shown.count
        let list = shown.joined(separator: "\n") + (more > 0 ? "\nand \(more) more" : "")
        let count = info.devices == 1 ? "1 device" : "\(info.devices) devices"
        return "This replaces the devices this Mac trusts with these \(count):\n\(list)\n\nOnly continue if you or someone you trust did the reset."
    }
}

extension Color {
    /// Orange for notices: darker in light mode so text keeps its contrast.
    static let noticeOrange = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? .systemOrange : NSColor(red: 0.68, green: 0.30, blue: 0, alpha: 1)
    })
}
