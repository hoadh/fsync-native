import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Files and transfers: offers waiting for the user, the transfer table, send and clear actions.
public struct TransfersPageView: View {
    @ObservedObject private var transfers: TransfersModel
    @ObservedObject private var files: FilesSync
    @ObservedObject private var devices: DevicesModel
    @State private var sendMessage: String?

    public init(transfers: TransfersModel, files: FilesSync, devices: DevicesModel) {
        self.transfers = transfers
        self.files = files
        self.devices = devices
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Files and transfers").font(.title2).accessibilityAddTraits(.isHeader)
            if let message = files.message { PageBanner(text: message, onDismiss: files.dismissMessage) }
            if let sendMessage { PageBanner(text: sendMessage) { self.sendMessage = nil } }
            if !files.offers.isEmpty { TransferOfferSection(files: files) }
            if transfers.rows.isEmpty {
                if files.offers.isEmpty {
                    ContentUnavailableView {
                        Label("No transfers yet", systemImage: "arrow.up.arrow.down.circle")
                    } description: {
                        Text("Drop files on a device in Devices.")
                    }
                } else {
                    Spacer(minLength: 0)
                }
            } else {
                TransfersTableView(transfers: transfers, files: files)
            }
        }
        .padding(20)
        .toolbar {
            ToolbarItem { SendFilesMenu(devices: devices.devices, files: files, message: $sendMessage) }
            ToolbarItem {
                Button("Clear finished") { transfers.clearFinished() }.disabled(transfers.finished.isEmpty)
            }
        }
    }
}
