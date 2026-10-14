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

    private static let log = Logger(subsystem: AppConfig.bundleId, category: "files")
    private let engine: FsyncEngineProtocol
    private let fetcher: FileFetchService
    private let downloads: URL
    private let autoAccept: () -> Bool
    private var running = false

    public init(engine: FsyncEngineProtocol, fetcher: FileFetchService, downloads: URL = FilesSync.defaultDownloads(),
                autoAccept: @escaping () -> Bool) {
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
    }

    /// `on_incoming_files`: download now when the user allows it, otherwise wait for `accept`.
    public func receive(offer: ClipItem) async {
        guard running, !offer.files.isEmpty else { return }
        if autoAccept() { await accept(offer) } else { offers.removeAll { $0.itemId == offer.itemId }; offers.insert(offer, at: 0) }
    }

    /// Downloads every file of the offer into the Downloads folder; the transfer list shows progress.
    public func accept(_ offer: ClipItem) async {
        offers.removeAll { $0.itemId == offer.itemId }
        var failed = 0
        for file in offer.files {
            if case .failure(let error) = await fetcher.fetch(itemId: offer.itemId, file: file, into: downloads) {
                if error != .cancelled { failed += 1 }
                Self.log.info("download failed: \(String(describing: error), privacy: .private)")
            }
        }
        message = failed == 0 ? nil : "\(failed) of \(offer.files.count) files couldn't be received from \(PeerText.sanitize(offer.fromName))."
    }

    public func decline(_ offer: ClipItem) { offers.removeAll { $0.itemId == offer.itemId } }

    /// "Send to…" / drop on a device: pushes `urls` to the device. The receiver must accept.
    public func send(_ urls: [URL], to nodeId: String) async {
        let paths = urls.filter(\.isFileURL).map(\.path)
        guard running, !paths.isEmpty else { return }
        let result = await EngineCall.run(engine) { try $0.sendFiles(nodeId: nodeId, paths: paths) }
        if case .failure(let error) = result { message = ErrorText.message(for: error, context: .general) } else { message = nil }
    }

    public func dismissMessage() { message = nil }
}
