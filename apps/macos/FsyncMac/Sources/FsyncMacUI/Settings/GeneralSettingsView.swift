import FsyncMacCore
import SwiftUI

/// Launch at login.
struct GeneralSettingsView: View {
    @ObservedObject var launch: LaunchAtLogin

    var body: some View {
        Form {
            Section {
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
        }
        .formStyle(.grouped)
        .onAppear { launch.refresh() }
    }
}
