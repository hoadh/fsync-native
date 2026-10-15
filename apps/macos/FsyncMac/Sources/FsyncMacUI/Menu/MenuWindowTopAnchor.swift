import AppKit
import SwiftUI

/// Keeps the menubar popover attached under its icon when its content changes height.
///
/// The `MenuBarExtra` window resizes with `setContentSize`, which keeps the bottom-left origin: when
/// content shrinks (an offer accepted, a message dismissed) the top edge drops away from the menubar.
/// This puts the top edge back after any resize that kept the origin; real moves are left alone.
struct MenuWindowTopAnchor: NSViewRepresentable {
    func makeNSView(context: Context) -> TopAnchorView { TopAnchorView() }
    func updateNSView(_ nsView: TopAnchorView, context: Context) {}
}

final class TopAnchorView: NSView {
    private var observers: [NSObjectProtocol] = []
    /// The window frame after the last move or correction.
    private var lastFrame: NSRect?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
        guard let window else { return lastFrame = nil }
        lastFrame = window.frame
        // `queue: nil` runs the block inside the post, so the top is restored before the frame is drawn.
        // AppKit posts window notifications on the main thread.
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: nil) { [weak self] _ in
            MainActor.assumeIsolated { self?.windowResized() }
        })
        observers.append(center.addObserver(forName: NSWindow.didMoveNotification, object: window, queue: nil) { [weak self] _ in
            MainActor.assumeIsolated { self?.lastFrame = self?.window?.frame }
        })
    }

    func windowResized() {
        guard let window else { return }
        let frame = window.frame
        defer { lastFrame = window.frame }
        guard let last = lastFrame, frame.origin == last.origin, frame.height != last.height else { return }
        window.setFrameOrigin(NSPoint(x: frame.minX, y: last.maxY - frame.height))
    }
    // No deinit cleanup needed: leaving the window calls `viewDidMoveToWindow` with nil, which removes them.
}
