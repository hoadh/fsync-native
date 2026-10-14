import AppKit

/// Copies a pairing code so clipboard managers and our own clipboard sync skip it
/// (nspasteboard.org conventions).
public enum PairingPasteboard {
    public static let concealedType = NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType")
    public static let transientType = NSPasteboard.PasteboardType("org.nspasteboard.TransientType")

    @discardableResult
    public static func copy(_ code: String, to pasteboard: NSPasteboard = .general) -> Bool {
        pasteboard.clearContents()
        // The marker types carry no data; their presence is the signal.
        return pasteboard.setString(code, forType: .string)
            && pasteboard.setData(Data(), forType: concealedType)
            && pasteboard.setData(Data(), forType: transientType)
    }
}
