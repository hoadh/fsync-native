import FsyncMacCore
import SwiftUI

/// The main window: one screen per `AppPhase`. A ready Mac gets the sidebar shell; every gating screen fills the window.
public struct RootView: View {
    @ObservedObject private var bridge: EngineBridge

    public init(bridge: EngineBridge) {
        self.bridge = bridge
    }

    public var body: some View {
        Group {
            switch bridge.phase {
            case .booting:
                BootingView(banner: bridge.banner)
            case .signedOut:
                if let login = bridge.loginModel {
                    LoginFlowView(model: login, banner: bridge.banner)
                } else {
                    BootingView(banner: nil)
                }
            case .needsPairing:
                gated { NeedsPairingView(recovery: bridge.recoveryModel, pairing: bridge.pairingModel) }
            case .ready:
                gated {
                    if bridge.isFirstDevice {
                        FirstDeviceView(onDone: bridge.dismissFirstDevice)
                    } else if let devices = bridge.devicesModel, let pairing = bridge.pairingModel,
                              let files = bridge.filesSync, let transfers = bridge.transfers {
                        MainSplitView(
                            navigation: bridge.navigation, devices: devices, files: files, transfers: transfers,
                            history: bridge.historyModel, sync: bridge.clipSync, pairing: pairing, banner: bridge.banner,
                            signOut: { Task { await bridge.signOut() } })
                    } else {
                        BootingView(banner: nil)
                    }
                }
            case .bootFailed(let message, let canReset):
                BootFailedView(message: message, canReset: canReset) { await bridge.startFresh() }
            }
        }
        .frame(minWidth: 620, minHeight: 420)
    }

    /// Trust-reset prompt takes over the window while one is pending.
    @ViewBuilder private func gated<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        if let trust = bridge.trustModel { TrustGate(trust: trust) { content() } } else { content() }
    }
}

struct BootingView: View {
    let banner: String?

    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(banner ?? "Starting fsync…").foregroundStyle(.secondary)
        }
        .padding(40)
    }
}

struct BootFailedView: View {
    let message: String
    let canReset: Bool
    let startFresh: () async -> Void
    @State private var confirming = false

    var body: some View {
        VStack(spacing: 14) {
            Text("fsync couldn't start").font(.headline)
            Text(message).foregroundStyle(.secondary).multilineTextAlignment(.center)
            if canReset {
                Text("Your clipboard history can't be opened with this Mac's saved key. Starting fresh moves the old history aside and pairs this Mac again.")
                    .font(.callout).multilineTextAlignment(.center)
                Button("Start fresh…", role: .destructive) { confirming = true }
                    .confirmationDialog("Start fresh?", isPresented: $confirming) {
                        Button("Cancel", role: .cancel) {}
                        Button("Move old history aside", role: .destructive) { Task { await startFresh() } }
                    } message: {
                        Text("The old history is kept as a backup file, but this Mac must be paired again.")
                    }
            }
        }
        .padding(30)
    }
}
