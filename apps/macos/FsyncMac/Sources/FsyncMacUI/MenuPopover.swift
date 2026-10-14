import FsyncMacCore
import SwiftUI

/// Menubar popover. Text entry lives in the main window, so signed-out users are sent there.
public struct MenuPopover: View {
    @ObservedObject private var bridge: EngineBridge
    private let openMain: () -> Void
    private let openSettings: () -> Void

    public init(bridge: EngineBridge, openMain: @escaping () -> Void, openSettings: @escaping () -> Void) {
        self.bridge = bridge
        self.openMain = openMain
        self.openSettings = openSettings
    }

    public var body: some View {
        Group {
            if bridge.phase == .ready, let devices = bridge.devicesModel, let trust = bridge.trustModel {
                MenuBarContentView(
                    devices: devices, trust: trust, banner: bridge.banner, openMain: openMain, openSettings: openSettings,
                    addDevice: { bridge.pairingModel?.openJoin(); openMain() },
                    signOut: { Task { await bridge.signOut() } })
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Text(statusText).font(.headline)
                    if let banner = bridge.banner { Text(banner).font(.caption).foregroundStyle(.secondary) }
                    switch bridge.phase {
                    case .signedOut: Button("Sign in…", action: openMain)
                    case .needsPairing: Button("Set up this Mac…", action: openMain)
                    default: EmptyView()
                    }
                    Divider()
                    Button("Settings…", action: openSettings)
                    Button("Quit fsync") { NSApplication.shared.terminate(nil) }
                }
            }
        }
        .padding(14)
        .frame(width: 280)
    }

    private var statusText: String {
        switch bridge.phase {
        case .booting: return "Starting…"
        case .signedOut: return "Signed out"
        case .needsPairing: return "Needs pairing"
        case .ready: return "Ready"
        case .bootFailed: return "Couldn't start"
        }
    }
}
