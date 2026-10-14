import Combine
import FsyncCoreKit
import Foundation

public struct TransferRow: Identifiable, Equatable, Sendable {
    public let id: UInt64
    public var itemId: String
    public var direction: TransferDirection
    public var state: TransferState
    public var bytesDone: UInt64
    public var bytesTotal: UInt64
    public var path: String?
    public var message: String?
    /// File name for display; set by whoever started the transfer.
    public var name: String?

    public var isActive: Bool { state == .started || state == .progress }
    public var fraction: Double { bytesTotal == 0 ? 0 : min(1, Double(bytesDone) / Double(bytesTotal)) }
}

public enum TransferOutcome: Equatable, Sendable {
    case done(path: String?)
    case failed(String)
    case cancelled
}

/// Progress of file transfers, fed by `on_transfer` events. Also answers "how did transfer N end?" for
/// callers that wait (eager download before a paste, lazy paste provider).
@MainActor
public final class TransfersModel: ObservableObject {
    public static let maxRows = 50

    @Combine.Published public private(set) var rows: [TransferRow] = []
    private let engine: FsyncEngineProtocol?
    private var names: [UInt64: String] = [:]

    public init(engine: FsyncEngineProtocol? = nil) { self.engine = engine }

    public var active: [TransferRow] { rows.filter(\.isActive) }

    public func handle(_ event: TransferEvent) {
        var row = rows.first { $0.id == event.transferId } ?? TransferRow(
            id: event.transferId, itemId: event.itemId, direction: event.direction, state: event.state, bytesDone: 0,
            bytesTotal: 0, path: nil, message: nil, name: names[event.transferId])
        row.state = event.state
        row.bytesDone = event.bytesDone
        row.bytesTotal = event.bytesTotal
        row.path = event.path ?? row.path
        row.message = event.message ?? row.message
        if let i = rows.firstIndex(where: { $0.id == row.id }) { rows[i] = row } else {
            rows.insert(row, at: 0)
            if rows.count > Self.maxRows, let last = rows.lastIndex(where: { !$0.isActive }) { rows.remove(at: last) }
        }
    }

    /// Names a transfer; works before or after its first event.
    public func setName(_ name: String, for id: UInt64) {
        names[id] = name
        if let i = rows.firstIndex(where: { $0.id == id }) { rows[i].name = name }
    }

    /// nil while the transfer runs (or is unknown).
    public func outcome(of id: UInt64) -> TransferOutcome? {
        guard let row = rows.first(where: { $0.id == id }) else { return nil }
        switch row.state {
        case .started, .progress: return nil
        case .done: return .done(path: row.path)
        case .failed: return .failed(row.message ?? "failed")
        case .cancelled: return .cancelled
        }
    }

    /// Waits for the end of transfer `id`; `timeout` seconds without an end counts as a failure.
    public func waitOutcome(of id: UInt64, timeout: TimeInterval = 600) async -> TransferOutcome {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let outcome = outcome(of: id) { return outcome }
            try? await Task.sleep(nanoseconds: 30_000_000)
        }
        return .failed("timed out")
    }

    public func cancel(_ id: UInt64) async {
        guard let engine else { return }
        _ = await EngineCall.run(engine) { try $0.cancel(transferId: id) }
    }

    public func clearFinished() { rows.removeAll { !$0.isActive } }

    public func reset() {
        rows = []
        names = [:]
    }
}
