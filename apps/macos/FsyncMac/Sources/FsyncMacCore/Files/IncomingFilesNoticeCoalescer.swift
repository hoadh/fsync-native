import Foundation

/// Rate limit for incoming-files notices: per sender, at most one emit per `window` seconds. Offers inside
/// the window are folded into one updated notice (`offerCount`) that `flush` hands out once the window ends.
/// Pure state machine: the caller supplies the time and owns the timer (see `nextDeadline`).
public struct IncomingFilesNoticeCoalescer {
    public static let defaultWindow: TimeInterval = 10

    private struct Sender {
        var windowStart: Date
        /// Aggregated notice waiting for the end of the window; nil when nothing was held back.
        var pending: IncomingFilesNotice?
    }

    private let window: TimeInterval
    private var senders: [String: Sender] = [:]

    public init(window: TimeInterval = defaultWindow) { self.window = window }

    /// The notice to show now, or nil when it was held back for the current window.
    public mutating func submit(_ notice: IncomingFilesNotice, at now: Date) -> IncomingFilesNotice? {
        // Idle senders cost nothing: forget those whose window is over and that hold nothing back.
        senders = senders.filter { $0.value.pending != nil || now.timeIntervalSince($0.value.windowStart) < window }
        // The key is the sanitized display name (`IncomingFilesNotice.fromName`); the offer carries no better id.
        let key = notice.fromName
        guard var sender = senders[key], now.timeIntervalSince(sender.windowStart) < window else {
            senders[key] = Sender(windowStart: now, pending: nil)
            return notice
        }
        // Only held-back offers are counted, like their files and bytes; the one already shown is not.
        var merged = notice
        if let held = sender.pending {
            merged.offerCount = held.offerCount + 1
            merged.fileCount += held.fileCount
            merged.totalBytes += held.totalBytes
        }
        sender.pending = merged
        senders[key] = sender
        return nil
    }

    /// Held-back notices whose window has ended; each starts a new window for its sender.
    public mutating func flush(at now: Date) -> [IncomingFilesNotice] {
        var out: [IncomingFilesNotice] = []
        for (key, sender) in senders.sorted(by: { $0.value.windowStart < $1.value.windowStart }) {
            guard let pending = sender.pending, now.timeIntervalSince(sender.windowStart) >= window else { continue }
            out.append(pending)
            senders[key] = Sender(windowStart: now, pending: nil)
        }
        return out
    }

    /// When `flush` next has something to hand out; nil if nothing is held back.
    public var nextDeadline: Date? {
        senders.values.filter { $0.pending != nil }.map { $0.windowStart.addingTimeInterval(window) }.min()
    }
}
