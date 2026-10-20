import AppKit
import Combine

/// Asks the hidden opener window to show Settings.
///
/// The opener window may not exist yet when the first request is made (the caller creates it with
/// `openWindow` first), so a request is also kept as pending until the opener consumes it: either when
/// it receives the signal, or when it appears. A request older than `maxAge` is dropped, so an opener
/// that appears much later never shows Settings unprompted.
@MainActor
public enum SettingsRequest {
    public static let maxAge: TimeInterval = 2
    private static let subject = PassthroughSubject<Void, Never>()
    private static var pendingSince: Date?

    public static var publisher: AnyPublisher<Void, Never> { subject.eraseToAnyPublisher() }

    public static func send(now: Date = Date()) {
        pendingSince = now
        subject.send()
    }

    /// True once per `send()` made within `maxAge`; the opener shows Settings when it returns true.
    public static func takePending(now: Date = Date()) -> Bool {
        defer { pendingSince = nil }
        guard let since = pendingSince else { return false }
        return now.timeIntervalSince(since) <= maxAge
    }
}

/// How the opener window hides itself.
public enum OpenerWindowStyle {
    /// Invisible, unclickable, unfocusable, absent from the Window menu and the window cycle.
    @MainActor public static func hide(_ window: NSWindow) {
        window.styleMask = [.borderless]  // a borderless window cannot become key or main
        window.alphaValue = 0
        window.ignoresMouseEvents = true
        window.isExcludedFromWindowsMenu = true
        window.collectionBehavior = [.transient, .ignoresCycle, .stationary]
        window.hasShadow = false
        window.isReleasedWhenClosed = false
        window.setFrame(NSRect(x: -10_000, y: -10_000, width: 1, height: 1), display: false)
        window.orderBack(nil)
    }
}
