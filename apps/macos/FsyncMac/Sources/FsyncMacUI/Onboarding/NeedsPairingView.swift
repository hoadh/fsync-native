import FsyncMacCore
import SwiftUI

/// This Mac is not trusted yet: it shows a pairing code for a trusted device to enter, and keeps
/// the recovery path for users who have no other device.
public struct NeedsPairingView: View {
    private let recovery: AccountRecoveryModel?
    private let pairing: PairingModel?

    public init(recovery: AccountRecoveryModel?, pairing: PairingModel?) {
        self.recovery = recovery
        self.pairing = pairing
    }

    public var body: some View {
        VStack(spacing: 14) {
            Text("Pair this Mac").font(.title2)
            Text("On a device you already use, choose Add device and enter this code.")
                .foregroundStyle(.secondary).multilineTextAlignment(.center)
            if let pairing { PairingFlowView(model: pairing, role: .offer) }
            if let recovery { RecoveryLink(model: recovery) }
        }
        .padding(.horizontal, 30).padding(.vertical, 20)
    }
}

private struct RecoveryLink: View {
    @ObservedObject var model: AccountRecoveryModel

    var body: some View {
        Button("Lost your other devices?") { model.begin() }
            .buttonStyle(.link)
            .sheet(isPresented: Binding(get: { model.isPresented }, set: { if !$0 { model.cancel() } })) {
                RecoverySheet(model: model)
            }
    }
}
