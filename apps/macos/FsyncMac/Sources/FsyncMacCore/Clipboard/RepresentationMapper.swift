import AppKit
import FsyncCoreKit

/// NSPasteboard <-> engine representations (MIME). Text wins over images: spreadsheets and rich
/// editors put a bitmap next to the text, and sending both wastes the budget.
public enum RepresentationMapper {
    public static let mimeText = "text/plain"
    public static let mimeHtml = "text/html"
    public static let mimeRtf = "text/rtf"
    public static let mimePng = "image/png"
    static let previewChars = 200

    public enum ClipRead: Equatable {
        case clip(LocalClip)
        case empty
        /// Sum of the representations, over the limit.
        case tooLarge(bytes: Int)
    }

    /// What the pasteboard holds now, as a clip. Files take precedence over text.
    public static func read(_ pb: NSPasteboard, maxBytes: UInt64) -> ClipRead {
        let files = fileURLs(pb)
        if !files.isEmpty {
            let names = files.map(\.lastPathComponent).joined(separator: ", ")
            return .clip(LocalClip(reps: [], files: files.map(\.path), preview: String(names.prefix(previewChars)), sensitive: false))
        }
        var reps: [Rep] = []
        var preview: String?
        if let text = pb.string(forType: .string), !text.isEmpty {
            reps.append(Rep(mime: mimeText, data: Data(text.utf8)))
            preview = String(text.prefix(previewChars))
        }
        if !reps.isEmpty {
            if let html = pb.data(forType: .html), !html.isEmpty { reps.append(Rep(mime: mimeHtml, data: html)) }
            if let rtf = pb.data(forType: .rtf), !rtf.isEmpty { reps.append(Rep(mime: mimeRtf, data: rtf)) }
        } else if let png = pngData(pb) {
            reps.append(Rep(mime: mimePng, data: png))
            preview = "Image"
        }
        guard !reps.isEmpty else { return .empty }
        let total = reps.reduce(0) { $0 + $1.data.count }
        if UInt64(total) > maxBytes { return .tooLarge(bytes: total) }
        return .clip(LocalClip(reps: reps, files: [], preview: preview, sensitive: false))
    }

    /// Regular files only; folders are out of scope for the MVP.
    static func fileURLs(_ pb: NSPasteboard) -> [URL] {
        let urls = pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return urls.filter { url in
            var isDir: ObjCBool = false
            return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && !isDir.boolValue
        }
    }

    /// PNG bytes for a copied image: PNG as is, TIFF (screenshots, most apps) converted.
    static func pngData(_ pb: NSPasteboard) -> Data? {
        if let png = pb.data(forType: .png), !png.isEmpty { return png }
        guard let tiff = pb.data(forType: .tiff), let rep = NSBitmapImageRep(data: tiff) else { return nil }
        return rep.representation(using: .png, properties: [:])
    }

    /// Pasteboard type for a representation we can apply, nil for anything else.
    public static func pasteboardType(forMime mime: String) -> NSPasteboard.PasteboardType? {
        switch mime {
        case mimeText: return .string
        case mimeHtml: return .html
        case mimeRtf: return .rtf
        case mimePng: return .png
        default: return nil
        }
    }

    /// The bytes as the pasteboard expects them for `type`.
    static func payload(_ data: Data, for type: NSPasteboard.PasteboardType) -> Data? {
        // Plain text must be valid UTF-8; anything else is dropped rather than pasted as garbage.
        if type == .string { return String(data: data, encoding: .utf8).map { Data($0.utf8) } }
        return data
    }
}
