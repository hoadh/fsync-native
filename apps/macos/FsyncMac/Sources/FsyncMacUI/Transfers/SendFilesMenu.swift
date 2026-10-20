import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// "Send files…": pick a device, then pick files. Only online devices are offered.
struct SendFilesMenu: View {
    let devices: [Device]
    let files: FilesSync
    @Binding var message: String?

    private var targets: [Device] { devices.filter { !$0.isSelf && $0.online } }

    var body: some View {
        Menu {
            ForEach(targets, id: \.nodeId) { device in
                Button(DevicesPageLogic.displayName(device)) { send(to: device) }
            }
        } label: {
            Label("Send files…", systemImage: "paperplane")
        }
        .disabled(targets.isEmpty)
        .help(targets.isEmpty ? "No device is online" : "Send files to a device")
    }

    private func send(to device: Device) {
        guard let urls = FilePicker.chooseFiles() else { return }
        switch DropSendRequest.validate(urls) {
        case .success(let valid):
            message = nil
            Task { await files.send(valid, to: device.nodeId) }
        case .failure(let error): message = DevicesPageLogic.text(for: error)
        }
    }
}
