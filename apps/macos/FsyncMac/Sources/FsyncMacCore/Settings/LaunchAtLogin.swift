import Combine
import Foundation
import ServiceManagement

/// Launch at login via `SMAppService.mainApp`. Only works from a real `.app` bundle; under
/// `swift run` the status is `.notFound` and the toggle is disabled.
@MainActor
public final class LaunchAtLogin: ObservableObject {
    public enum Status: Equatable, Sendable {
        case enabled
        case disabled
        /// Registered, but the user must approve it in System Settings.
        case requiresApproval
        /// Not running from a bundle (or the system doesn't know the app).
        case unavailable
    }

    @Combine.Published public private(set) var status: Status = .unavailable
    @Combine.Published public private(set) var errorMessage: String?

    public init() { refresh() }

    public static func map(_ raw: SMAppService.Status) -> Status {
        switch raw {
        case .enabled: return .enabled
        case .notRegistered: return .disabled
        case .requiresApproval: return .requiresApproval
        case .notFound: return .unavailable
        @unknown default: return .unavailable
        }
    }

    public var canToggle: Bool { status != .unavailable }
    public var isOn: Bool { status == .enabled || status == .requiresApproval }

    public func refresh() { status = Self.map(SMAppService.mainApp.status) }

    public func setEnabled(_ on: Bool) {
        errorMessage = nil
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            errorMessage = "Couldn't change launch at login: \(error.localizedDescription)"
        }
        refresh()
    }

    public func openSystemSettings() { SMAppService.openSystemSettingsLoginItems() }
}
