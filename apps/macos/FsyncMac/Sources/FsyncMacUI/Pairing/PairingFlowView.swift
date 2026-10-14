import FsyncMacCore
import SwiftUI

/// One screen per `PairingState`, for either role.
public struct PairingFlowView: View {
    @ObservedObject private var model: PairingModel
    private let role: PairingRole

    public init(model: PairingModel, role: PairingRole) {
        self.model = model
        self.role = role
    }

    public var body: some View {
        VStack(spacing: 14) {
            switch model.state {
            case .idle:
                if role == .join { PairJoinView(model: model) } else { PairOfferView(model: model) }
            case .offering:
                PairOfferView(model: model)
            case .joining:
                PairJoiningView(model: model)
            case .sas(let peer, _):
                SasConfirmView(peer: peer) { id, accept in Task { await model.confirm(requestId: id, accept: accept) } }
            case .waitingForPeer:
                ProgressView()
                Text("Waiting for the other device…").foregroundStyle(.secondary)
            case .done(let name):
                Image(systemName: "checkmark.circle.fill").font(.system(size: 36)).foregroundStyle(.green)
                Text(verbatim: "Paired with \(name)").font(.title3)
                Button("Done") { model.dismiss() }.keyboardShortcut(.defaultAction)
            case .failed(let text):
                Text("Pairing didn't finish").font(.headline)
                Text(verbatim: text).foregroundStyle(.secondary).multilineTextAlignment(.center)
                HStack {
                    if role == .join { Button("Cancel") { model.dismiss() } }
                    Button("Try again") { retry() }.keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(30)
        .onDisappear { Task { await model.cancel() } }
    }

    private func retry() {
        model.dismiss()
        if role == .join { model.openJoin() } else { Task { await model.offer(.code) } }
    }
}
