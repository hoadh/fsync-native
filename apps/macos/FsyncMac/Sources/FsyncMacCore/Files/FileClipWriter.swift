import AppKit
import FsyncCoreKit
import Foundation

/// Serves one remote file when a reader asks for its `public.file-url` (ADR 009 decision 1).
final class LazyFileProvider: NSObject, NSPasteboardItemDataProvider {
    private let resolve: @Sendable () -> URL?

    init(resolve: @escaping @Sendable () -> URL?) { self.resolve = resolve }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        guard type == .fileURL, let url = resolve() else { return }
        item.setString(url.absoluteString, forType: .fileURL)
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {}
}

/// Puts the files of a received clip on the pasteboard: fetched first (eager) or on paste (lazy), by
/// `FileClipStrategy`. Marked with `com.fsync.origin`. Folders arrive as flat files (not in the MVP).
@MainActor
public final class FileClipWriter {
    private let pasteboard: NSPasteboard
    private let fetcher: FileFetchService
    private let inbox: InboxDirectory
    private let strategy: () -> FileClipStrategy
    private var providers: [LazyFileProvider] = []
    public private(set) var lastWriteCount: Int?
    private var lastItemId: String?

    public init(pasteboard: NSPasteboard = .general, fetcher: FileFetchService, inbox: InboxDirectory,
                strategy: @escaping () -> FileClipStrategy = { FileClipStrategy.stored() }) {
        self.pasteboard = pasteboard
        self.fetcher = fetcher
        self.inbox = inbox
        self.strategy = strategy
    }

    public var isCurrent: Bool { lastWriteCount == pasteboard.changeCount }

    public func apply(_ clip: ClipItem) async -> Result<Void, ClipboardWriter.WriteError> {
        guard !clip.files.isEmpty else { return .failure(.nothingUsable) }
        let dir: URL
        do { dir = try inbox.folder(forItem: clip.itemId) } catch { return .failure(.fetchFailed) }
        let eager = strategy().eagerFlags(sizes: clip.files.map(\.size))
        var items: [NSPasteboardItem] = []
        var lazy: [LazyFileProvider] = []
        for (file, now) in zip(clip.files, eager) {
            let item = NSPasteboardItem()
            if now {
                guard case .success(let url) = await fetcher.fetch(itemId: clip.itemId, file: file, into: dir) else {
                    return .failure(.fetchFailed)
                }
                item.setString(url.absoluteString, forType: .fileURL)
            } else {
                let (fetcher, id) = (self.fetcher, clip.itemId)
                let provider = LazyFileProvider {
                    // AppKit calls providers on the main thread; anywhere else there is no safe way to wait.
                    guard Thread.isMainThread else { return nil }
                    return MainActor.assumeIsolated { fetcher.fetchBlockingOnMain(itemId: id, file: file, into: dir) }
                }
                item.setDataProvider(provider, forTypes: [.fileURL])
                lazy.append(provider)
            }
            item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.originType))
            items.append(item)
        }
        pasteboard.clearContents()
        providers = lazy
        guard pasteboard.writeObjects(items) else { return .failure(.fetchFailed) }
        lastWriteCount = pasteboard.changeCount
        lastItemId = clip.itemId
        return .success(())
    }

    /// Item folder that must not be cleaned up: its files may still be pasted.
    public var currentItemId: String? { isCurrent ? lastItemId : nil }

    /// On quit: a lazy entry cannot be served after we exit, so remove it if it is still current.
    public func clearIfOurs() {
        guard isCurrent, !providers.isEmpty else { return }
        pasteboard.clearContents()
        lastWriteCount = nil
        providers = []
    }
}
