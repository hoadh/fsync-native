import Combine
import FsyncCoreKit
import Foundation

/// The account's devices, kept fresh by `on_device_status` merges and reloads from the engine.
@MainActor
public final class DevicesModel: ObservableObject {
    @Combine.Published public private(set) var devices: [Device] = []
    @Combine.Published public private(set) var isBusy = false
    @Combine.Published public private(set) var errorMessage: String?

    private let engine: FsyncEngineProtocol
    private let onSignInRequired: @MainActor () -> Void

    public init(engine: FsyncEngineProtocol, onSignInRequired: @escaping @MainActor () -> Void) {
        self.engine = engine
        self.onSignInRequired = onSignInRequired
    }

    public var summary: StatusSummary { StatusSummary(devices) }

    /// Reads the list from the engine; fixes anything a missed event left stale.
    public func reload() async {
        let list = await Task.detached { [engine] in engine.devices() }.value
        devices = Self.sorted(list)
    }

    /// A status event carries the full record: replace by node id, or append an unknown device.
    public func merge(_ device: Device) {
        var list = devices
        if let i = list.firstIndex(where: { $0.nodeId == device.nodeId }) { list[i] = device } else { list.append(device) }
        devices = Self.sorted(list)
    }

    public func clear() {
        devices = []
        errorMessage = nil
    }

    public func dismissError() { errorMessage = nil }

    /// Removes the device with `nodeId`, the one the user confirmed. This Mac cannot be removed.
    public func remove(nodeId: String) async {
        guard !isBusy, let device = devices.first(where: { $0.nodeId == nodeId }), !device.isSelf else { return }
        isBusy = true
        errorMessage = nil
        let result = await EngineCall.run(engine) { try $0.removeDevice(nodeId: nodeId) }
        isBusy = false
        switch result {
        case .success: await reload()
        case .failure(let error):
            if ErrorText.requiresSignIn(error) { return onSignInRequired() }
            errorMessage = ErrorText.message(for: error, context: .general)
        }
    }

    /// Self first, then by name, then node id (stable).
    static func sorted(_ list: [Device]) -> [Device] {
        list.sorted {
            if $0.isSelf != $1.isSelf { return $0.isSelf }
            let order = $0.name.localizedCaseInsensitiveCompare($1.name)
            return order == .orderedSame ? $0.nodeId < $1.nodeId : order == .orderedAscending
        }
    }
}
