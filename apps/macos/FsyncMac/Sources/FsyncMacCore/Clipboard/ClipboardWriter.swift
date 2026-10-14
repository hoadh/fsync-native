import AppKit
import FsyncCoreKit

/// Fetches one representation of a received item (`fetch_rep`); blocking.
public typealias RepFetcher = @Sendable (_ itemId: String, _ index: UInt32) throws -> Data

/// Serves large representations only when a reader asks for them (like the lazy files of ADR 009).
final class LazyRepProvider: NSObject, NSPasteboardItemDataProvider {
    private let fetch: RepFetcher
    private let itemId: String
    /// Pasteboard type -> index of the representation in the item.
    private let indexes: [NSPasteboard.PasteboardType: UInt32]

    init(itemId: String, indexes: [NSPasteboard.PasteboardType: UInt32], fetch: @escaping RepFetcher) {
        self.itemId = itemId
        self.indexes = indexes
        self.fetch = fetch
    }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        guard let index = indexes[type] else { return }
        let (fetch, id) = (fetch, itemId)
        let data = BlockingFetch.run { try? fetch(id, index) }
        if let data, let payload = RepresentationMapper.payload(data, for: type) { item.setData(payload, forType: type) }
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {}
}

/// Writes a received clip to the pasteboard. Marked with `com.fsync.origin` so the watcher skips it.
@MainActor
public final class ClipboardWriter {
    /// Representations up to this size are fetched before the write; larger ones lazily (images).
    public static let eagerLimit: UInt64 = 1 << 20

    public enum WriteError: Error, Equatable { case nothingUsable, fetchFailed }

    private let pasteboard: NSPasteboard
    private let fetch: RepFetcher
    /// The lazy provider must outlive the pasteboard entry.
    private var provider: LazyRepProvider?
    /// changeCount right after our last write: that change is ours.
    public private(set) var lastWriteCount: Int?

    public init(pasteboard: NSPasteboard = .general, fetch: @escaping RepFetcher) {
        self.pasteboard = pasteboard
        self.fetch = fetch
    }

    /// True when the pasteboard still holds what this writer last put there.
    public var isCurrent: Bool { lastWriteCount == pasteboard.changeCount }

    public func apply(_ clip: ClipItem) async -> Result<Void, WriteError> {
        let usable = clip.reps.enumerated().compactMap { i, rep in
            RepresentationMapper.pasteboardType(forMime: rep.mime).map { (index: UInt32(i), type: $0, size: rep.size) }
        }
        guard !usable.isEmpty else { return .failure(.nothingUsable) }
        // Small representations now, off the main thread; at least one must arrive.
        let (fetch, id) = (fetch, clip.itemId)
        let eager = usable.filter { $0.size <= Self.eagerLimit }
        let loaded: [(NSPasteboard.PasteboardType, Data)] = await Task.detached {
            eager.compactMap { r in
                (try? fetch(id, r.index)).flatMap { RepresentationMapper.payload($0, for: r.type) }.map { (r.type, $0) }
            }
        }.value
        let lazy = usable.filter { $0.size > Self.eagerLimit }
        guard !loaded.isEmpty || !lazy.isEmpty else { return .failure(.fetchFailed) }

        let item = NSPasteboardItem()
        for (type, data) in loaded { item.setData(data, forType: type) }
        var newProvider: LazyRepProvider?
        if !lazy.isEmpty {
            let p = LazyRepProvider(itemId: id, indexes: Dictionary(lazy.map { ($0.type, $0.index) }) { a, _ in a }, fetch: fetch)
            item.setDataProvider(p, forTypes: lazy.map(\.type))
            newProvider = p
        }
        item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.originType))
        pasteboard.clearContents()
        provider = newProvider
        guard pasteboard.writeObjects([item]) else { return .failure(.fetchFailed) }
        lastWriteCount = pasteboard.changeCount
        return .success(())
    }

    /// Puts plain text on the pasteboard (history copy-back); marked like any write of ours.
    @discardableResult
    public func writeText(_ text: String) -> Bool {
        let item = NSPasteboardItem()
        item.setString(text, forType: .string)
        item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.originType))
        pasteboard.clearContents()
        provider = nil
        let ok = pasteboard.writeObjects([item])
        lastWriteCount = pasteboard.changeCount
        return ok
    }

    /// On quit: a lazy entry cannot be served after we exit, so remove it if it is still current.
    public func clearIfOurs() {
        guard isCurrent, provider != nil else { return }
        pasteboard.clearContents()
        lastWriteCount = nil
        provider = nil
    }
}
