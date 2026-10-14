import FsyncMacCore
import SwiftUI
import TrustResetPrompt

/// The popover for a trusted Mac: status, devices, actions. Dialogs live in the main window.
struct MenuBarContentView: View {
    @ObservedObject var devices: DevicesModel
    @ObservedObject var trust: TrustResetPromptModel
    let sync: ClipboardSync?
    var files: FilesSync?
    var transfers: TransfersModel?
    let banner: String?
    let openHistory: () -> Void
    let openMain: () -> Void
    let openSettings: () -> Void
    let addDevice: () -> Void
    let signOut: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            StatusHeaderView(summary: devices.summary)
            if let banner { Text(banner).font(.caption).foregroundStyle(.secondary) }
            if !trust.pending.isEmpty {
                Button("Review device list change…", action: openMain).foregroundStyle(.orange)
            }
            Divider()
            if let sync {
                ClipboardMenuSection(sync: sync, openHistory: openHistory)
                Divider()
            }
            if let files, let transfers {
                FilesMenuSection(files: files, transfers: transfers)
            }
            DeviceListView(devices: devices.devices, onRemove: nil, onSend: files.map { files in
                { device, urls in Task { await files.send(urls, to: device.nodeId) } }
            })
            Divider()
            Button("Add device…", action: addDevice)
            Button("Open fsync", action: openMain)
            Button("Settings…", action: openSettings)
            SignOutButton(action: signOut)
            Button("Quit fsync") { NSApplication.shared.terminate(nil) }
        }
        .onAppear { Task { await devices.reload() } }
    }
}
