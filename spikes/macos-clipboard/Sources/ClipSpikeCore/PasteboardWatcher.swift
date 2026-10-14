import AppKit

public struct PasteboardChange {
    public let changeCount: Int
    /// Type names only; content is never read by the watcher.
    public let types: [String]
    public let decision: FilterDecision
}

/// Polls `changeCount` (free: no privacy alert) and reads type names only. `poll()` is the unit that
/// tests call directly; `start` drives it from a timer with an adaptive interval.
public final class PasteboardWatcher {
    private let pasteboard: NSPasteboard
    private let filter: SensitiveFilter
    private let frontmost: () -> String?
    private let onChange: (PasteboardChange) -> Void
    private var lastCount: Int
    private var timer: Timer?
    private var lastChangeAt = Date.distantPast
    public private(set) var polls = 0

    public init(pasteboard: NSPasteboard = .general, filter: SensitiveFilter = SensitiveFilter(),
                frontmost: @escaping () -> String? = { NSWorkspace.shared.frontmostApplication?.bundleIdentifier },
                onChange: @escaping (PasteboardChange) -> Void) {
        self.pasteboard = pasteboard
        self.filter = filter
        self.frontmost = frontmost
        self.onChange = onChange
        self.lastCount = pasteboard.changeCount
    }

    /// One poll. Returns true when the pasteboard changed since the previous poll.
    @discardableResult
    public func poll() -> Bool {
        polls += 1
        let count = pasteboard.changeCount
        guard count != lastCount else { return false }
        lastCount = count
        lastChangeAt = Date()
        let types = (pasteboard.types ?? []).map(\.rawValue)
        onChange(PasteboardChange(changeCount: count, types: types,
                                  decision: filter.decide(types: types, frontmostBundleId: frontmost())))
        return true
    }

    /// Fixed interval (`fixed`), or adaptive: `fast` for `fastWindow` seconds after a change, else `idle`.
    public func start(fixed: TimeInterval? = nil, fast: TimeInterval = 0.15, idle: TimeInterval = 1.0,
                      fastWindow: TimeInterval = 3) {
        stop()
        schedule(fixed: fixed, fast: fast, idle: idle, fastWindow: fastWindow)
    }

    private func schedule(fixed: TimeInterval?, fast: TimeInterval, idle: TimeInterval, fastWindow: TimeInterval) {
        let wait = fixed ?? (Date().timeIntervalSince(lastChangeAt) < fastWindow ? fast : idle)
        timer = Timer.scheduledTimer(withTimeInterval: wait, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.poll()
            self.schedule(fixed: fixed, fast: fast, idle: idle, fastWindow: fastWindow)
        }
        timer?.tolerance = wait * 0.1
    }

    public func stop() { timer?.invalidate(); timer = nil }
}
