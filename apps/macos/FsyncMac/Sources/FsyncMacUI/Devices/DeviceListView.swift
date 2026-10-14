import FsyncCoreKit
import SwiftUI

struct DeviceListView: View {
    let devices: [Device]
    let onRemove: ((Device) -> Void)?
    var onSend: ((Device, [URL]) -> Void)?

    var body: some View {
        if devices.isEmpty {
            Text("No devices yet.").foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(devices, id: \.nodeId) { device in
                    DeviceRowView(device: device, onRemove: onRemove.map { remove in { remove(device) } },
                                  onSend: onSend.map { send in { send(device, $0) } })
                }
            }
        }
    }
}
