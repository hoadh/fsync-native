import AppKit

public struct PasteboardChange: Sendable {
    public let changeCount: Int
    /// Type names only; the watcher never reads content.
    public let types: [String]
    public let decision: FilterDecision
}

/// Polls `changeCount` (free: no privacy alert) and reads type names only. `poll()` is what tests
/// call; `start` drives it from a one-shot timer chain with an adaptive interval (ADR 009 decision 5).
@MainActor
public final class PasteboardWatcher {
    private let pasteboard: NSPasteboard
    private var filter: SensitiveFilter
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

    public var isRunning: Bool { timer != nil }

    public func update(filter: SensitiveFilter) { self.filter = filter }

    /// Forgets changes made so far (used when switching to auto mode: old content is not published).
    public func resync() { lastCount = pasteboard.changeCount }

    /// One poll. True when the pasteboard changed since the previous poll.
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

    /// Fast interval for `fastWindow` seconds after a change, else `idle`. `fixed` overrides both.
    public func start(fixed: TimeInterval? = nil, fast: TimeInterval = 0.15, idle: TimeInterval = 1.0,
                      fastWindow: TimeInterval = 3) {
        stop()
        resync()
        schedule(fixed: fixed, fast: fast, idle: idle, fastWindow: fastWindow)
    }

    public func stop() { timer?.invalidate(); timer = nil }

    private func schedule(fixed: TimeInterval?, fast: TimeInterval, idle: TimeInterval, fastWindow: TimeInterval) {
        let wait = fixed ?? (Date().timeIntervalSince(lastChangeAt) < fastWindow ? fast : idle)
        let next = Timer(timeInterval: wait, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.timer != nil else { return }
                self.poll()
                self.schedule(fixed: fixed, fast: fast, idle: idle, fastWindow: fastWindow)
            }
        }
        next.tolerance = wait * 0.1
        // .common keeps polling while a menu or the popover is tracking events.
        RunLoop.main.add(next, forMode: .common)
        timer = next
    }
}
