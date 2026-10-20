import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// The clipboard history page: search, copy again, pin, delete, clear.
public struct HistoryView: View {
    /// What the "Clear…" dialog was opened for; the dialog answers for this choice only.
    private enum ClearChoice: Identifiable {
        case keepPinned, everything
        var id: Self { self }
    }

    @ObservedObject private var history: HistoryModel
    /// Live device list: rows show device names, and a rename shows up without reloading history.
    @ObservedObject private var devices: DevicesModel
    private let sync: ClipboardSync?
    @State private var search = ""
    @State private var clearing: ClearChoice?

    public init(history: HistoryModel, devices: DevicesModel, sync: ClipboardSync?) {
        self.history = history
        self.devices = devices
        self.sync = sync
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Clipboard history").font(.title2).accessibilityAddTraits(.isHeader)
            if let error = history.errorMessage { PageBanner(text: error, onDismiss: history.dismissError) }
            content
            Text("Copy again restores the full text or image of a clip; file clips can't be restored.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(20)
        .searchable(text: $search, prompt: "Search history")
        .toolbar {
            ToolbarItem {
                Menu {
                    Button("Clear all but pinned…") { clearing = .keepPinned }
                    Button("Clear everything…", role: .destructive) { clearing = .everything }
                } label: { Label("Clear…", systemImage: "trash") }
                .disabled(history.items.isEmpty)
            }
        }
        .task(id: search) {
            // The bridge already loads the list on navigation and on new clips: only act on a changed query.
            guard search != history.query else { return }
            // Debounce; a new keystroke cancels this task.
            if !search.isEmpty { try? await Task.sleep(nanoseconds: 250_000_000) }
            guard !Task.isCancelled else { return }
            await history.reload(query: search)
        }
        .confirmationDialog(
            clearing == .everything ? "Clear everything?" : "Clear all but pinned?",
            isPresented: Binding(get: { clearing != nil }, set: { if !$0 { clearing = nil } }),
            titleVisibility: .visible, presenting: clearing
        ) { choice in
            Button("Cancel", role: .cancel) {}
            Button(choice == .everything ? "Clear everything" : "Clear all but pinned", role: .destructive) {
                Task { await history.clear(keepPinned: choice == .keepPinned) }
            }
        } message: { choice in
            Text(choice == .everything ? "Pinned clips go too. This only affects this Mac." : "This only affects this Mac.")
        }
    }

    @ViewBuilder private var content: some View {
        if history.items.isEmpty {
            if history.isLoading {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if search.isEmpty {
                ContentUnavailableView("Nothing copied yet.", systemImage: "clock.arrow.circlepath")
            } else {
                ContentUnavailableView.search(text: search)
            }
        } else {
            List {
                ForEach(history.items, id: \.id) { item in
                    HistoryRowView(
                        item: item,
                        peerName: PeerText.deviceName(peer: item.peer, in: devices.devices),
                        onCopy: copy(item),
                        onPin: { Task { await history.setPinned(item.id, !item.pinned) } },
                        onDelete: { Task { await history.delete(item.id) } })
                }
                if history.hasMore {
                    Button("Load more") { Task { await history.loadMore() } }.buttonStyle(.link)
                }
            }
        }
    }

    private func copy(_ item: HistoryItem) -> (() -> Void)? {
        guard item.canCopyBack, let sync else { return nil }
        return { Task { _ = await sync.copyBack(itemId: item.id) } }
    }
}
