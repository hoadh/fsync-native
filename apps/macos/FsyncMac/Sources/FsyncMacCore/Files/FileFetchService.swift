import AppKit
import FsyncCoreKit
import Foundation

/// Calls `fetch_file_to`; returns the transfer id (the end is reported by `on_transfer`).
public typealias FileFetchStart = @Sendable (_ itemId: String, _ fileId: UInt32, _ destDir: String) throws -> UInt64

public enum FileFetchError: Error, Equatable {
    case failed(String)
    case cancelled
}

/// Downloads one file of a received item and returns where it landed. `fetch_file_to` returns at once
/// with a transfer id; the end arrives as a `Done` / `Failed` / `Cancelled` event in `TransfersModel`.
@MainActor
public final class FileFetchService {
    private let transfers: TransfersModel
    private let start: FileFetchStart

    public init(transfers: TransfersModel, start: @escaping FileFetchStart) {
        self.transfers = transfers
        self.start = start
    }

    public convenience init(engine: FsyncEngineProtocol, transfers: TransfersModel) {
        self.init(transfers: transfers) { [engine] item, file, dir in
            try engine.fetchFileTo(itemId: item, fileId: file, destDir: dir)
        }
    }

    public func fetch(itemId: String, file: FileInfo, into dir: URL) async -> Result<URL, FileFetchError> {
        let (start, dirPath) = (start, dir.path)
        let started = await Task.detached { Result { try start(itemId, file.fileId, dirPath) } }.value
        switch started {
        case .failure(let error): return .failure(.failed(ErrorText.message(for: error, context: .general) ?? "cancelled"))
        case .success(let id):
            transfers.setName(file.name, for: id)
            return Self.result(await transfers.waitOutcome(of: id), file: file, dir: dir)
        }
    }

    /// For a pasteboard data provider, which AppKit calls on the main thread while the reader waits. The
    /// main run loop keeps turning, so events (and the menu) are processed during the wait (ADR 009).
    public func fetchBlockingOnMain(itemId: String, file: FileInfo, into dir: URL,
                                    timeout: TimeInterval = 600) -> URL? {
        let (start, dirPath) = (start, dir.path)
        guard let id = BlockingFetch.run({ try? start(itemId, file.fileId, dirPath) }) else { return nil }
        transfers.setName(file.name, for: id)
        let deadline = Date().addingTimeInterval(timeout)
        while transfers.outcome(of: id) == nil, Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        guard let outcome = transfers.outcome(of: id) else { return nil }
        if case .success(let url) = Self.result(outcome, file: file, dir: dir) { return url }
        return nil
    }

    private static func result(_ outcome: TransferOutcome, file: FileInfo, dir: URL) -> Result<URL, FileFetchError> {
        switch outcome {
        case .cancelled: return .failure(.cancelled)
        case .failed(let message): return .failure(.failed(message))
        case .done(let path):
            let url = path.map { URL(fileURLWithPath: $0) } ?? dir.appendingPathComponent(file.name)
            return FileManager.default.fileExists(atPath: url.path) ? .success(url) : .failure(.failed("file missing"))
        }
    }
}
