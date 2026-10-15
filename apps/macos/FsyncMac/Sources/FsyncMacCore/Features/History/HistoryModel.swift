import Combine
import FsyncCoreKit
import Foundation

extension HistoryItem {
    /// The engine can return the full content of every item that is not files-only.
    public var canCopyBack: Bool { kind != .files }
}

/// The clipboard history list: search, paging, pin, delete and clear. Content stays in the engine.
@MainActor
public final class HistoryModel: ObservableObject {
    public static let defaultPageSize: UInt32 = 50

    @Combine.Published public private(set) var items: [HistoryItem] = []
    @Combine.Published public private(set) var hasMore = false
    @Combine.Published public private(set) var isLoading = false
    @Combine.Published public private(set) var errorMessage: String?
    @Combine.Published public private(set) var query = ""

    private let engine: FsyncEngineProtocol
    private let pageSize: UInt32
    private var cursor: String?
    /// Bumped by every reload; tests read it to see that a reload was asked for.
    private(set) var generation = 0
    private var refreshQueued = false

    public init(engine: FsyncEngineProtocol, pageSize: UInt32 = HistoryModel.defaultPageSize) {
        self.engine = engine
        self.pageSize = max(pageSize, 1)
    }

    /// Loads the first page for `query` (empty = everything); a newer call supersedes an older one.
    public func reload(query newQuery: String? = nil) async {
        if let newQuery { query = newQuery }
        generation += 1
        let mine = generation
        cursor = nil
        await load(append: false, generation: mine)
    }

    /// Re-reads the list after new items arrived, keeping as many rows as are loaded now so the user's
    /// place survives. Calls made in the same turn (one clip raises several events) share one read.
    public func refresh() async {
        guard !refreshQueued else { return }
        refreshQueued = true
        let seen = generation
        await Task.yield()
        refreshQueued = false
        guard seen == generation else { return }  // a reset or reload ran meanwhile
        generation += 1
        cursor = nil
        await load(append: false, generation: generation, limit: max(pageSize, UInt32(clamping: items.count)))
    }

    public func loadMore() async {
        guard hasMore, !isLoading else { return }
        await load(append: true, generation: generation)
    }

    private func load(append: Bool, generation mine: Int, limit: UInt32? = nil) async {
        let text = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let from = append ? cursor : nil
        let limit = limit ?? pageSize
        isLoading = true
        let result = await EngineCall.run(engine) {
            try $0.history(query: text.isEmpty ? nil : text, cursor: from, limit: limit)
        }
        guard mine == generation else { return }
        isLoading = false
        switch result {
        case .success(let page):
            items = append ? items + page.items : page.items
            cursor = page.nextCursor
            hasMore = page.nextCursor != nil
            errorMessage = nil
        case .failure(let error):
            errorMessage = ErrorText.message(for: error, context: .general)
        }
    }

    public func setPinned(_ id: String, _ pinned: Bool) async {
        await mutate { try $0.pin(itemId: id, pinned: pinned) }
    }

    public func delete(_ id: String) async {
        await mutate { try $0.delete(itemId: id) }
    }

    /// Removes everything except pinned items when `keepPinned`.
    public func clear(keepPinned: Bool = true) async {
        await mutate { try $0.clear(keepPinned: keepPinned) }
    }

    public func dismissError() { errorMessage = nil }

    public func reset() {
        generation += 1
        items = []
        cursor = nil
        hasMore = false
        isLoading = false
        errorMessage = nil
    }

    private func mutate(_ body: @escaping @Sendable (FsyncEngineProtocol) throws -> Void) async {
        let result = await EngineCall.run(engine, body)
        if case .failure(let error) = result {
            errorMessage = ErrorText.message(for: error, context: .general)
            return
        }
        await reload()
    }
}
