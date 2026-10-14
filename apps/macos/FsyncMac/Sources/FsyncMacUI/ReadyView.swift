import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Main window for a trusted Mac: the device list with remove, plus the add-device flow.
struct ReadyView: View {
    @ObservedObject var devices: DevicesModel
    @ObservedObject var pairing: PairingModel
    let banner: String?
    let signOut: () -> Void
    /// The device the user is being asked about; the answer applies to this node id only.
    @State private var removing: Device?

    var body: some View {
        if pairing.isActive {
            PairingFlowView(model: pairing, role: .join)
        } else {
            home
        }
    }

    private var home: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Your devices").font(.title2)
            if let banner { Text(banner).font(.callout).foregroundStyle(.secondary) }
            if let error = devices.errorMessage {
                Text(error).font(.callout).foregroundStyle(.red).onTapGesture { devices.dismissError() }
            }
            DeviceListView(devices: devices.devices) { removing = $0 }
            Spacer(minLength: 0)
            HStack {
                Button("Add device…") { pairing.openJoin() }
                Spacer()
                SignOutButton(action: signOut)
            }
        }
        .padding(24)
        .onAppear { Task { await devices.reload() } }
        .confirmationDialog(
            Text(verbatim: "Remove ‘\(PeerText.sanitize(removing?.name ?? ""))’?"),
            isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
            titleVisibility: .visible, presenting: removing
        ) { device in
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) { Task { await devices.remove(nodeId: device.nodeId) } }
        } message: { _ in
            Text("It will stop syncing with this account and must be paired again.")
        }
    }
}
