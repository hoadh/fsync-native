import Foundation

/// Display helpers for the pairing code.
public enum PairingCode {
    /// Groups of four for reading aloud; the original string is what gets copied and joined with.
    public static func grouped(_ code: String) -> String {
        let chars = code.filter { !$0.isWhitespace && $0 != "-" }
        return stride(from: 0, to: chars.count, by: 4).map { start in
            let from = chars.index(chars.startIndex, offsetBy: start)
            let to = chars.index(from, offsetBy: min(4, chars.count - start))
            return String(chars[from..<to])
        }.joined(separator: " ")
    }

    /// Whole seconds left until `expiresMs` (Unix ms), never negative.
    public static func secondsLeft(expiresMs: UInt64, now: Date = Date()) -> Int {
        let nowMs = UInt64(max(0, now.timeIntervalSince1970 * 1000))
        return expiresMs > nowMs ? Int((expiresMs - nowMs + 999) / 1000) : 0
    }
}
