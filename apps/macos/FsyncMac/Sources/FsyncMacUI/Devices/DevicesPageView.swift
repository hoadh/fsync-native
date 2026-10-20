import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Devices page: this account's devices, send by drop or "Send files…", remove with confirmation.
public struct DevicesPageView: View {
    @ObservedObject private var devices: DevicesModel
    @ObservedObject private var files: FilesSync
    private let onPair: () -> Void

    /// The device the user is being asked about; the answer applies to this node id only.
    @State private var removing: Device?
    @State private var pending: DropRequest?
    @State private var targeted: String?
    @State private var dropMessage: String?

    public init(devices: DevicesModel, files: FilesSync, onPair: @escaping () -> Void) {
        self.devices = devices
        self.files = files
        self.onPair = onPair
    }

    private var others: [Device] { devices.devices.filter { !$0.isSelf } }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            banners
            if others.isEmpty {
                ContentUnavailableView {
                    Label("No other devices yet", systemImage: "laptopcomputer.and.iphone")
                } description: {
                    Text("Pair one to start syncing.")
                } actions: {
                    Button("Pair device…", action: onPair)
                }
            } else {
                list
            }
        }
        .padding(20)
        .task { await devices.reload() }
        // Closes by itself if the target device leaves the list while the sheet is up.
        .sheet(item: Binding(
            get: { pending.flatMap { DevicesPageLogic.isPresented(nodeId: $0.nodeId, in: devices.devices) ? $0 : nil } },
            set: { pending = $0 })
        ) { request in
            DropSendConfirmation(request: request, onSend: { confirmSend(request) }, onCancel: { pending = nil })
        }
        .confirmationDialog(
            Text(verbatim: "Remove \(removing.map(DevicesPageLogic.displayName) ?? "")?"),
            isPresented: Binding(get: { isRemoveDialogPresented }, set: { if !$0 { removing = nil } }),
            titleVisibility: .visible, presenting: removing
        ) { device in
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) { Task { await devices.remove(nodeId: device.nodeId) } }
        } message: { _ in
            Text("It will stop syncing with this account and must be paired again.")
        }
    }

    /// Closes if the device vanished from the list while the dialog was open.
    private var isRemoveDialogPresented: Bool {
        DevicesPageLogic.isPresented(nodeId: removing?.nodeId, in: devices.devices)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Text("Your devices").font(.title2).accessibilityAddTraits(.isHeader)
                Text("\(devices.devices.count)").foregroundStyle(.secondary).monospacedDigit()
            }
            Text("Drop files on a device to send them.").font(.callout).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder private var banners: some View {
        if let error = devices.errorMessage { PageBanner(text: error, onDismiss: devices.dismissError) }
        if let message = files.message { PageBanner(text: message, onDismiss: files.dismissMessage) }
        if let dropMessage { PageBanner(text: dropMessage) { self.dropMessage = nil } }
    }

    private var list: some View {
        List(devices.devices, id: \.nodeId) { device in
            DevicePageRow(
                device: device, isDropTargeted: targeted == device.nodeId,
                onSend: { chooseFiles(for: device) }, onRemove: { removing = device }
            )
            .dropDestination(for: URL.self) { urls, _ in
                guard !device.isSelf else { return false }
                return handleDrop(urls, on: device)
            } isTargeted: { isOver in
                // No highlight where a drop can't land (this Mac, offline devices).
                if isOver, DevicesPageLogic.acceptsDrop(device) { targeted = device.nodeId }
                else if targeted == device.nodeId { targeted = nil }
            }
        }
    }

    /// "Send files…": the open panel is the confirmation, so the picked files go out after validation.
    private func chooseFiles(for device: Device) {
        guard let urls = FilePicker.chooseFiles() else { return }
        switch DropSendRequest.validate(urls) {
        case .success(let valid):
            dropMessage = nil
            Task { await files.send(valid, to: device.nodeId) }
        case .failure(let error): dropMessage = DevicesPageLogic.text(for: error)
        }
    }

    /// Returns whether the drop was accepted for confirmation.
    private func handleDrop(_ urls: [URL], on device: Device) -> Bool {
        switch DevicesPageLogic.decide(urls: urls, on: device) {
        case .confirm(let request):
            dropMessage = nil
            pending = request
            return true
        case .reject(let message):
            dropMessage = message
            return false
        }
    }

    /// Files can change between the drop and the click: check them again before sending.
    private func confirmSend(_ request: DropRequest) {
        pending = nil
        switch DropSendRequest.validate(request.urls) {
        case .success(let valid):
            dropMessage = nil
            Task { await files.send(valid, to: request.nodeId) }
        case .failure(let error): dropMessage = DevicesPageLogic.text(for: error)
        }
    }
}
