import Combine
import Foundation

public enum MainSection: String, CaseIterable, Hashable, Sendable {
    case devices, history, transfers
}

/// Which page the single main window shows, and whether the pairing sheet is up.
///
/// `section` is optional because `List(selection:)` needs a `Binding<MainSection?>`; the detail view maps
/// `nil` to `.devices`, and `show(_:)` always stores a non-nil value. Every assignment publishes, so the
/// bridge refreshes the history on each `show(.history)` (the list may be stale after a long stay elsewhere).
@MainActor
public final class MainNavigation: ObservableObject {
    @Published public var section: MainSection? = .devices
    @Published public private(set) var isPairingSheetPresented = false

    public init() {}

    public func show(_ section: MainSection) { self.section = section }
    public func presentPairing() { isPairingSheetPresented = true }
    public func dismissPairing() { isPairingSheetPresented = false }

    /// Back to the first page with no sheet; used on sign-out and when the app leaves `.ready`.
    public func reset() {
        section = .devices
        isPairingSheetPresented = false
    }
}
