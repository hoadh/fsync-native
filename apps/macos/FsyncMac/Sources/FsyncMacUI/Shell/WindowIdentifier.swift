import AppKit
import SwiftUI

/// `NSWindow.identifier` values the app looks windows up by (titles can be localised or change).
/// Phase 05 (notifications) reuses these.
public enum WindowIDs {
    /// The main window (`Window("fsync", id: SceneIDs.main)`).
    public static let main = NSUserInterfaceItemIdentifier("fsync.main")
    /// The invisible helper window that owns `openSettings`.
    public static let settingsOpener = NSUserInterfaceItemIdentifier("fsync.settings-opener")

    /// SwiftUI scene ids (what `openWindow(id:)` takes); not the same as the AppKit identifiers above.
    public enum SceneIDs {
        public static let main = "main"
        public static let settingsOpener = "settings-opener"
    }

    /// The opener is hidden by restyling it, after which SwiftUI puts its scene id back as the identifier.
    /// Either value means "the opener".
    @MainActor public static func isSettingsOpener(_ window: NSWindow) -> Bool {
        window.identifier == settingsOpener || window.identifier?.rawValue == SceneIDs.settingsOpener
    }

    /// The live window with this identifier, if SwiftUI has created it.
    @MainActor public static func window(_ id: NSUserInterfaceItemIdentifier) -> NSWindow? {
        NSApp.windows.first { $0.identifier == id }
    }
}

/// Stamps the hosting `NSWindow` with an identifier once the view is in a window.
public struct WindowIdentifier: NSViewRepresentable {
    private let id: NSUserInterfaceItemIdentifier
    /// Runs once the window exists (the opener uses it to hide itself).
    private let configure: (NSWindow) -> Void

    public init(_ id: NSUserInterfaceItemIdentifier, configure: @escaping (NSWindow) -> Void = { _ in }) {
        self.id = id
        self.configure = configure
    }

    public func makeNSView(context: Context) -> IdentifierView { IdentifierView(id: id, configure: configure) }
    public func updateNSView(_ nsView: IdentifierView, context: Context) {}

    public final class IdentifierView: NSView {
        private let id: NSUserInterfaceItemIdentifier
        private let configure: (NSWindow) -> Void

        init(id: NSUserInterfaceItemIdentifier, configure: @escaping (NSWindow) -> Void) {
            self.id = id
            self.configure = configure
            super.init(frame: .zero)
        }

        @available(*, unavailable) required init?(coder: NSCoder) { fatalError("not used") }

        public override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window else { return }
            // Stamp last: restyling a window (the opener's borderless switch) makes SwiftUI reset the identifier.
            configure(window)
            window.identifier = id
        }
    }
}
