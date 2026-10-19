import Combine
import FsyncCoreKit
import Foundation
import os

/// Files pushed by other devices ("Send to…") and files this Mac sends.
@MainActor
public final class FilesSync: ObservableObject {
    /// Offers waiting for the user (auto-accept off). Newest first.
    @Combine.Published public private(set) var offers: [ClipItem] = []
    @Combine.Published public private(set) var message: String?

    /// Offers waiting for the user are capped; the oldest are dropped.
    public static let maxOffers = 20
    /// Auto-accept only below this many bytes per offer; bigger ones wait for the user.
    public static let autoAcceptLimit: UInt64 = 1 << 30

    private static let log = Logger(subsystem: AppConfig.bundleId, category: "files")
    private let engine: FsyncEngineProtocol
    private let fetcher: FileFetchService
    private let downloads: URL
    private let autoAccept: () -> Bool
    private var running = false
    /// Called after files were offered to a device (the engine recorded them in history).
    var onSent: (() -> Void)?
    /// New incoming offers, sanitized and rate-limited per sender (the bridge forwards them to the UI).
    var onNotice: ((IncomingFilesNotice) -> Void)?
    /// Whether the device is still one of the account's trusted devices; a retry needs it. Wired by the bridge.
    var isPeerTrusted: (String) -> Bool = { _ in false }
    private let now: () -> Date
    private let noticeWindow: TimeInterval
    private var coalescer: IncomingFilesNoticeCoalescer
    private var flushTask: Task<Void, Never>?
    /// Offers already announced, so a repeated offer (same item) does not notify twice.
    private var noticedIds: [String] = []
    /// `(itemId, fileId)` of retries under way: a double tap starts one download.
    private var retrying: Set<String> = []

    public init(engine: FsyncEngineProtocol, fetcher: FileFetchService, downloads: URL = FilesSync.defaultDownloads(),
                now: @escaping () -> Date = Date.init, noticeWindow: TimeInterval = IncomingFilesNoticeCoalescer.defaultWindow,
                autoAccept: @escaping () -> Bool) {
        self.now = now
        self.noticeWindow = noticeWindow
        self.coalescer = IncomingFilesNoticeCoalescer(window: noticeWindow)
        self.engine = engine
        self.fetcher = fetcher
        self.downloads = downloads
        self.autoAccept = autoAccept
    }

    public static func defaultDownloads() -> URL {
        FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
    }

    public func start() { running = true }

    public func stop() {
        running = false
        offers = []
        flushTask?.cancel()
        flushTask = nil
        coalescer = IncomingFilesNoticeCoalescer(window: noticeWindow)
        noticedIds = []
        retrying = []
        fetcher.clearSpecs()
    }

    /// Retry is only offered for peers that are still trusted; see `isPeerTrusted`.
    func pruneRetrySpecs(keepingPeers peers: Set<String>) { fetcher.pruneSpecs(keepingPeers: peers) }
    func clearRetrySpecs() { fetcher.clearSpecs() }

    /// `on_incoming_files`: download now when the user allows it, otherwise wait for `accept`.
    public func receive(offer: ClipItem) async {
        guard running, !offer.files.isEmpty else { return }
        // Decided before `offers` changes: a repeated offer is not announced again.
        let isNew = !offers.contains { $0.itemId == offer.itemId } && !noticedIds.contains(offer.itemId)
        let auto = autoAccept() && offer.totalSize <= Self.autoAcceptLimit
        if isNew { announce(offer, autoAccepted: auto) }
        if auto { await accept(offer); return }
        offers.removeAll { $0.itemId == offer.itemId }
        offers.insert(offer, at: 0)
        if offers.count > Self.maxOffers { offers.removeLast(offers.count - Self.maxOffers) }
    }

    /// Downloads every file of the offer into the Downloads folder; the transfer list shows progress.
    public func accept(_ offer: ClipItem) async {
        offers.removeAll { $0.itemId == offer.itemId }
        var failed = 0
        for file in offer.files {
            if case .failure(let error) = await fetcher.fetch(itemId: offer.itemId, file: file, into: downloads, peerNodeId: offer.from) {
                Self.log.info("download failed: \(String(describing: error), privacy: .private)")
                // The user cancelled: the rest of the offer is not wanted either.
                if error == .cancelled { break }
                failed += 1
            }
        }
        message = failed == 0 ? nil : "\(failed) of \(offer.files.count) files couldn't be received from \(PeerText.sanitize(offer.fromName))."
    }

    private func announce(_ offer: ClipItem, autoAccepted: Bool) {
        noticedIds.append(offer.itemId)
        if noticedIds.count > 100 { noticedIds.removeFirst() }
        let notice = IncomingFilesNotice(fromName: offer.fromName, fileCount: offer.files.count, totalBytes: offer.totalSize,
                                         autoAccepted: autoAccepted)
        if let now = coalescer.submit(notice, at: now()) { onNotice?(now) } else { scheduleFlush() }
    }

    /// Emits the notices held back by the coalescer once their window ends.
    private func scheduleFlush() {
        guard flushTask == nil, coalescer.nextDeadline != nil else { return }
        flushTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, let deadline = self.coalescer.nextDeadline else { self?.flushTask = nil; return }
                let wait = deadline.timeIntervalSince(self.now())
                // At least 50 ms per turn, so a deadline that is already due can never busy-loop the main actor.
                try? await Task.sleep(nanoseconds: UInt64(max(wait, 0.05) * 1_000_000_000))
                guard !Task.isCancelled else { return }
                self.coalescer.flush(at: self.now()).forEach { self.onNotice?($0) }
            }
        }
    }

    /// A failed receive can be started again when its spec is known, the sender is still trusted and the
    /// same file is not already being retried.
    public func canRetry(_ row: TransferRow) -> Bool {
        guard row.state == .failed, row.direction == .receive, let spec = fetcher.retrySpec(for: row.id) else { return false }
        return isPeerTrusted(spec.peerNodeId) && !retrying.contains(Self.key(spec))
    }

    /// Starts the failed download again as a new transfer; the failed row is replaced. Repeated taps while
    /// the retry runs are dropped.
    public func retry(_ row: TransferRow) async {
        guard running, canRetry(row), let spec = fetcher.retrySpec(for: row.id) else { return }
        let key = Self.key(spec)
        retrying.insert(key)
        defer { retrying.remove(key) }
        if case .failure(let error) = await fetcher.fetch(itemId: spec.itemId, file: spec.file, into: spec.destDir,
                                                          peerNodeId: spec.peerNodeId, replacing: row.id),
           error != .cancelled {
            Self.log.info("retry failed: \(String(describing: error), privacy: .private)")
            message = "\(PeerText.sanitize(spec.file.name)) couldn't be received."
        }
    }

    private static func key(_ spec: FileFetchService.RetrySpec) -> String { "\(spec.itemId)/\(spec.file.fileId)" }

    public func decline(_ offer: ClipItem) { offers.removeAll { $0.itemId == offer.itemId } }

    /// "Send to…" / drop on a device: pushes `urls` to the device. The receiver must accept.
    public func send(_ urls: [URL], to nodeId: String) async {
        let paths = urls.filter(\.isFileURL).map(\.path)
        guard running, !paths.isEmpty else { return }
        let result = await EngineCall.run(engine) { try $0.sendFiles(nodeId: nodeId, paths: paths) }
        switch result {
        case .failure(let error): message = ErrorText.message(for: error, context: .general)
        case .success:
            message = nil
            onSent?()
        }
    }

    public func dismissMessage() { message = nil }
}
