import FsyncMacCore
import SwiftUI

/// Menubar popover (width 280). Text entry lives in the main window, so signed-out users are sent there.
public struct MenuPopover: View {
    @ObservedObject private var bridge: EngineBridge
    private let openMain: () -> Void
    private let openSettings: () -> Void
    private let openHistory: () -> Void
    private let openTransfers: () -> Void
    private let addDevice: () -> Void

    public init(bridge: EngineBridge, openMain: @escaping () -> Void, openSettings: @escaping () -> Void,
                openHistory: @escaping () -> Void = {}, openTransfers: @escaping () -> Void = {},
                addDevice: @escaping () -> Void = {}) {
        self.openTransfers = openTransfers
        self.addDevice = addDevice
        self.bridge = bridge
        self.openMain = openMain
        self.openSettings = openSettings
        self.openHistory = openHistory
    }

    public var body: some View {
        Group {
            if bridge.phase == .ready, let devices = bridge.devicesModel, let trust = bridge.trustModel {
                MenuBarContentView(
                    devices: devices, trust: trust, sync: bridge.clipSync, files: bridge.filesSync,
                    transfers: bridge.transfers, banner: bridge.banner, openHistory: openHistory, openTransfers: openTransfers,
                    openMain: openMain, openSettings: openSettings,
                    addDevice: addDevice,
                    signOut: { Task { await bridge.signOut() } })
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Text(statusText).font(.headline)
                    if let banner = bridge.banner { Text(banner).font(.caption).foregroundStyle(.secondary) }
                    switch bridge.phase {
                    case .signedOut: Button("Sign in…", action: openMain).buttonStyle(MenuRowButtonStyle())
                    case .needsPairing: Button("Set up this Mac…", action: openMain).buttonStyle(MenuRowButtonStyle())
                    default: EmptyView()
                    }
                    Divider()
                    VStack(alignment: .leading, spacing: 0) {
                        Button("Settings…", action: openSettings)
                        Button("Quit fsync") { NSApplication.shared.terminate(nil) }
                    }
                    .buttonStyle(MenuRowButtonStyle())
                }
            }
        }
        .padding(14)
        .frame(width: 280)
        .background(MenuWindowTopAnchor())
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
