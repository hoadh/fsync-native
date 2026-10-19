import FsyncCoreKit
import FsyncMacCore
import SwiftUI
import AppKit

/// Transfers as a table: name, direction, size, progress, status, and per-row actions.
struct TransfersTableView: View {
    @ObservedObject var transfers: TransfersModel
    @ObservedObject var files: FilesSync
    /// Rows whose retry call is still running; keeps the button disabled meanwhile.
    @State private var retrying: Set<UInt64> = []

    var body: some View {
        Table(transfers.rows) {
            TableColumn("Name") { row in
                Text(verbatim: Self.name(row)).lineLimit(1).truncationMode(.middle)
            }
            TableColumn("Direction") { row in
                Label(row.direction == .receive ? "Received" : "Sent",
                      systemImage: row.direction == .receive ? "arrow.down" : "arrow.up")
            }
            .width(min: 90, ideal: 100)
            TableColumn("Size") { row in
                Text(verbatim: row.bytesTotal == 0 ? "—" : ByteCountFormatter.string(fromByteCount: Int64(clamping: row.bytesTotal), countStyle: .file))
                    .foregroundStyle(.secondary).monospacedDigit()
            }
            .width(min: 60, ideal: 80)
            TableColumn("Progress") { row in
                if row.isActive {
                    ProgressView(value: row.fraction)
                        .accessibilityLabel("Progress of \(Self.name(row))")
                        .accessibilityValue("\(Int(row.fraction * 100)) percent")
                } else { Text("") }
            }
            .width(min: 80, ideal: 120)
            TableColumn("Status") { row in
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: Self.status(row.state))
                    // The reason a transfer failed is shown, not hidden in a tooltip.
                    if row.state == .failed, let reason = Self.failureText(row) {
                        Text(verbatim: reason).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    }
                }
                .help(Self.failureText(row) ?? "")
            }
            .width(min: 90, ideal: 150)
            TableColumn("") { row in actions(row) }.width(min: 150, ideal: 190)
        }
    }

    @ViewBuilder private func actions(_ row: TransferRow) -> some View {
        HStack(spacing: 8) {
            if row.isActive {
                Button("Cancel") { Task { await transfers.cancel(row.id) } }
                    .accessibilityLabel("Cancel \(Self.name(row))")
            }
            if row.state == .failed, row.direction == .receive {
                Button("Retry") { retry(row) }.disabled(!files.canRetry(row) || retrying.contains(row.id))
                    .accessibilityLabel("Retry \(Self.name(row))")
            }
            if let url = row.revealURL {
                Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([url]) }
                    .accessibilityLabel("Show \(Self.name(row)) in Finder")
            }
        }
        .buttonStyle(.borderless)
    }

    private func retry(_ row: TransferRow) {
        guard retrying.insert(row.id).inserted else { return }
        Task {
            await files.retry(row)
            retrying.remove(row.id)
        }
    }

    static func name(_ row: TransferRow) -> String {
        let clean = row.name.map { PeerText.sanitize($0, limit: 60) } ?? ""
        return clean.isEmpty ? "File" : clean
    }

    /// The engine's failure text, sanitized; nil when there is none.
    static func failureText(_ row: TransferRow) -> String? {
        guard let message = row.message else { return nil }
        let clean = PeerText.sanitize(message, limit: 120)
        return clean.isEmpty ? nil : clean
    }

    static func status(_ state: TransferState) -> String {
        switch state {
        case .started, .progress: return "In progress"
        case .done: return "Done"
        case .failed: return "Failed"
        case .cancelled: return "Cancelled"
        }
    }
}
