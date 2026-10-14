import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// The clipboard history window: search, copy again, pin, delete, clear.
public struct HistoryView: View {
    @ObservedObject private var history: HistoryModel
    private let sync: ClipboardSync?
    @State private var search = ""
    @State private var confirmClear = false

    public init(history: HistoryModel, sync: ClipboardSync?) {
        self.history = history
        self.sync = sync
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                TextField("Search history", text: $search).textFieldStyle(.roundedBorder)
                Button("Clear…") { confirmClear = true }.disabled(history.items.isEmpty)
            }
            if let error = history.errorMessage {
                Text(error).font(.callout).foregroundStyle(.red).onTapGesture { history.dismissError() }
            }
            if history.items.isEmpty {
                Spacer()
                Text(search.isEmpty ? "Nothing copied yet." : "No matches.").foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                Spacer()
            } else {
                List {
                    ForEach(history.items, id: \.id) { item in
                        HistoryRowView(
                            item: item,
                            onCopy: copy(item),
                            onPin: { Task { await history.setPinned(item.id, !item.pinned) } },
                            onDelete: { Task { await history.delete(item.id) } })
                    }
                    if history.hasMore {
                        Button("Load more") { Task { await history.loadMore() } }.buttonStyle(.link)
                    }
                }
            }
            Text("Copying again restores the first 200 characters of text clips.").font(.caption).foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(minWidth: 460, minHeight: 360)
        .task(id: search) {
            // Debounce; a new keystroke cancels this task.
            if !search.isEmpty { try? await Task.sleep(nanoseconds: 250_000_000) }
            guard !Task.isCancelled else { return }
            await history.reload(query: search)
        }
        .confirmationDialog("Clear history?", isPresented: $confirmClear, titleVisibility: .visible) {
            Button("Cancel", role: .cancel) {}
            Button("Clear all but pinned", role: .destructive) { Task { await history.clear(keepPinned: true) } }
            Button("Clear everything", role: .destructive) { Task { await history.clear(keepPinned: false) } }
        } message: {
            Text("This only affects this Mac.")
        }
    }

    private func copy(_ item: HistoryItem) -> (() -> Void)? {
        guard item.kind == .text, let text = item.preview, let sync else { return nil }
        return { sync.copyBack(text: text) }
    }
}
