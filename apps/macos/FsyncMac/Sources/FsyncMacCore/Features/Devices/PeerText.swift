import FsyncCoreKit
import Foundation

/// Names and reasons that come from another device are untrusted text.
public enum PeerText {
    /// Drops control and format characters (newlines, bidi overrides), trims, and caps the length.
    public static func sanitize(_ text: String, limit: Int = 40) -> String {
        let cleaned = String(String.UnicodeScalarView(text.unicodeScalars.filter {
            $0.properties.generalCategory != .control && $0.properties.generalCategory != .format
        })).trimmingCharacters(in: .whitespaces)
        guard cleaned.count > limit else { return cleaned }
        return String(cleaned.prefix(limit)) + "…"
    }

    /// The name shown for a history row's peer: the paired device's name, or a neutral fallback when
    /// the row has no peer or the device was removed. Never the raw node id.
    public static func deviceName(peer: String?, in devices: [Device]) -> String {
        guard let peer, let device = devices.first(where: { $0.nodeId == peer }) else { return "another device" }
        let name = sanitize(device.name)
        return name.isEmpty ? "another device" : name
    }
}
