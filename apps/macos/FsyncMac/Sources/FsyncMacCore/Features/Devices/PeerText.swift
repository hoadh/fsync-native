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
}
