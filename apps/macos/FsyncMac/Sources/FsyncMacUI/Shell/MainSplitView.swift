import FsyncMacCore
import SwiftUI

/// The main window for a trusted Mac: sidebar of pages, the page, and the pairing sheet.
struct MainSplitView: View {
    @ObservedObject var navigation: MainNavigation
    @ObservedObject var devices: DevicesModel
    @ObservedObject var files: FilesSync
    let transfers: TransfersModel
    let history: HistoryModel?
    let sync: ClipboardSync?
    let pairing: PairingModel
    let banner: String?
    let signOut: () -> Void

    var body: some View {
        NavigationSplitView {
            SidebarView(navigation: navigation, files: files, onPair: navigation.presentPairing, signOut: signOut)
        } detail: {
            VStack(spacing: 0) {
                if let banner {
                    Text(verbatim: banner).font(.callout).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading).padding([.horizontal, .top], 20)
                }
                page
            }
            .navigationTitle(SidebarView.title(navigation.section ?? .devices))
            .toolbar { MainToolbar(summary: devices.summary, onPair: navigation.presentPairing) }
        }
        .sheet(isPresented: Binding(
            get: { navigation.isPairingSheetPresented },
            set: { if !$0 { navigation.dismissPairing() } }
        )) {
            PairingSheet(model: pairing, onClose: navigation.dismissPairing)
        }
    }

    /// `nil` selection shows Devices (the list can clear its selection).
    @ViewBuilder private var page: some View {
        switch navigation.section ?? .devices {
        case .devices:
            DevicesPageView(devices: devices, files: files, onPair: navigation.presentPairing)
        case .history:
            if let history {
                HistoryView(history: history, devices: devices, sync: sync)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        case .transfers:
            TransfersPageView(transfers: transfers, files: files, devices: devices)
        }
    }
}
