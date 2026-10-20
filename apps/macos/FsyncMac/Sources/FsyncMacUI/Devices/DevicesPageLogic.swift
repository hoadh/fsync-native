import FsyncCoreKit
import FsyncMacCore
import Foundation

/// One dropped item, measured once when the request is built (not on every redraw).
struct DropItem: Equatable {
    let url: URL
    let name: String
    let isDirectory: Bool
    let sizeText: String

    init(url: URL) {
        self.url = url
        name = PeerText.sanitize(url.lastPathComponent, limit: 60)
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .fileSizeKey])
        isDirectory = values?.isDirectory == true
        sizeText = isDirectory ? "Folder" : ByteCountFormatter.string(fromByteCount: Int64(values?.fileSize ?? 0), countStyle: .file)
    }
}

/// What the user dropped on a device and still has to confirm.
struct DropRequest: Identifiable, Equatable {
    let id = UUID()
    let nodeId: String
    let deviceName: String
    let items: [DropItem]

    var urls: [URL] { items.map(\.url) }
}

enum DropDecision: Equatable {
    case confirm(DropRequest)
    case reject(String)
}

/// Pure decisions of the Devices page, kept out of the view so they can be tested.
enum DevicesPageLogic {
    static func displayName(_ device: Device) -> String {
        let clean = PeerText.sanitize(device.name)
        return clean.isEmpty ? "another device" : clean
    }

    /// Only another device that is online takes a drop (and shows the drop highlight).
    static func acceptsDrop(_ device: Device) -> Bool { !device.isSelf && device.online }

    /// A dialog opened for `nodeId` stays up only while that device is still in the list.
    static func isPresented(nodeId: String?, in devices: [Device]) -> Bool {
        guard let nodeId else { return false }
        return devices.contains { $0.nodeId == nodeId }
    }

    /// Validate what was dropped, then ask for confirmation; nothing is sent here.
    static func decide(urls: [URL], on device: Device) -> DropDecision {
        guard !device.isSelf else { return .reject("This Mac can't send files to itself.") }
        guard device.online else {
            return .reject("\(displayName(device)) is offline. Try again when it is online.")
        }
        switch DropSendRequest.validate(urls) {
        case .success(let valid):
            return .confirm(DropRequest(nodeId: device.nodeId, deviceName: displayName(device), items: valid.map(DropItem.init)))
        case .failure(let error): return .reject(text(for: error))
        }
    }

    static func text(for error: DropError) -> String {
        switch error {
        case .empty: return "Nothing to send."
        case .notFileURL: return "Only files and folders from this Mac can be sent."
        case .missing(let name): return "“\(name)” no longer exists."
        }
    }
}
