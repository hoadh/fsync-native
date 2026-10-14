import FsyncMacCore
import SwiftUI

/// Settings scene: General (launch at login) and Account (sign out, node id).
public struct SettingsView: View {
    @ObservedObject private var bridge: EngineBridge
    @StateObject private var launch = LaunchAtLogin()

    public init(bridge: EngineBridge) { self.bridge = bridge }

    public var body: some View {
        TabView {
            GeneralSettingsView(launch: launch)
                .tabItem { Label("General", systemImage: "gearshape") }
            AccountSettingsView(bridge: bridge)
                .tabItem { Label("Account", systemImage: "person.crop.circle") }
        }
        .frame(width: 420, height: 230)
    }
}

struct GeneralSettingsView: View {
    @ObservedObject var launch: LaunchAtLogin

    var body: some View {
        Form {
            Toggle("Launch fsync at login", isOn: Binding(get: { launch.isOn }, set: { launch.setEnabled($0) }))
                .disabled(!launch.canToggle)
            if launch.status == .unavailable {
                Text("Available when fsync runs as an installed app (FsyncMac.app).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if launch.status == .requiresApproval {
                Text("Approve fsync under Login Items in System Settings.").font(.caption).foregroundStyle(.secondary)
                Button("Open System Settings", action: launch.openSystemSettings)
            }
            if let error = launch.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .padding(20)
        .onAppear { launch.refresh() }
    }
}

struct AccountSettingsView: View {
    @ObservedObject var bridge: EngineBridge
    @State private var nodePrefix: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(bridge.phase == .signedOut ? "Signed out" : "Signed in").font(.headline)
            if let nodePrefix { Text(verbatim: "This Mac: \(nodePrefix)…").font(.caption).foregroundStyle(.secondary) }
            SignOutButton(isDisabled: bridge.phase == .signedOut || bridge.phase == .booting) {
                Task { await bridge.signOut() }
            }
            Text("Your device keys and clipboard history stay on this Mac.").font(.caption).foregroundStyle(.secondary)
            Spacer(minLength: 0)
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .task { nodePrefix = await bridge.nodeIdPrefix() }
    }
}
