import FsyncCoreKit

/// Counts of other devices by connection state. This Mac is not counted.
///
/// The core reports Lan/Relay only for a live QUIC connection, which exists once something syncs.
/// A device the backend sees online but with no connection yet has `status == .offline` and
/// `online == true`; it counts as `present`, not offline.
public struct StatusSummary: Equatable, Sendable {
    public let lan: Int
    public let relay: Int
    public let present: Int
    public let offline: Int

    public init(_ devices: [Device]) {
        let others = devices.filter { !$0.isSelf }
        lan = others.filter { $0.status == .lan }.count
        relay = others.filter { $0.status == .relay }.count
        present = others.filter { $0.status == .offline && $0.online }.count
        offline = others.filter { $0.status == .offline && !$0.online }.count
    }

    /// Reachable by any means: connected or seen online by the backend.
    public var online: Int { lan + relay + present }
    public var total: Int { online + offline }

    /// "Connected" only for a live LAN/relay link; presence alone is "Online".
    public var headline: String {
        if lan + relay > 0 { return "Connected" }
        return present > 0 ? "Online" : "No devices online"
    }

    public var text: String {
        if total == 0 { return "No other devices yet" }
        let presence = present > 0 ? " · \(present) online" : ""
        return "\(lan) on LAN · \(relay) via relay\(presence) · \(offline) offline"
    }
}
