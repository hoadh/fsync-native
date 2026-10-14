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

    /// The raw bytes of what the pasteboard holds, read on the main thread with no conversion. `build`
    /// does the expensive part (TIFF to PNG) and is safe to run anywhere.
    public struct Snapshot: Sendable {
        let files: [URL]
        let text: String?
        let html: Data?
        let rtf: Data?
        let png: Data?
        let tiff: Data?

        /// A TIFF this much over the limit is refused without decoding it (PNG is rarely under 1/16).
        static let tiffRefusalFactor: UInt64 = 16

        public func build(maxBytes: UInt64) -> ClipRead {
            if !files.isEmpty {
                let names = files.map(\.lastPathComponent).joined(separator: ", ")
                return .clip(LocalClip(reps: [], files: files.map(\.path), preview: String(names.prefix(RepresentationMapper.previewChars)), sensitive: false))
            }
            var reps: [Rep] = []
            var preview: String?
            if let text, !text.isEmpty {
                reps.append(Rep(mime: mimeText, data: Data(text.utf8)))
                preview = String(text.prefix(RepresentationMapper.previewChars))
                if let html { reps.append(Rep(mime: mimeHtml, data: html)) }
                if let rtf { reps.append(Rep(mime: mimeRtf, data: rtf)) }
            } else if let png {
                reps.append(Rep(mime: mimePng, data: png))
                preview = "Image"
            } else if let tiff {
                if UInt64(tiff.count) > maxBytes &* Self.tiffRefusalFactor { return .tooLarge(bytes: tiff.count) }
                guard let converted = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { return .empty }
                reps.append(Rep(mime: mimePng, data: converted))
                preview = "Image"
            }
            guard !reps.isEmpty else { return .empty }
            let total = reps.reduce(0) { $0 + $1.data.count }
            if UInt64(total) > maxBytes { return .tooLarge(bytes: total) }
            return .clip(LocalClip(reps: reps, files: [], preview: preview, sensitive: false))
        }
    }

    /// Files take precedence over text, text over images. Image bytes are read only when there is no text.
    public static func snapshot(_ pb: NSPasteboard) -> Snapshot {
        let files = fileURLs(pb)
        guard files.isEmpty else { return Snapshot(files: files, text: nil, html: nil, rtf: nil, png: nil, tiff: nil) }
        func data(_ type: NSPasteboard.PasteboardType) -> Data? { pb.data(forType: type).flatMap { $0.isEmpty ? nil : $0 } }
        if let text = pb.string(forType: .string), !text.isEmpty {
            return Snapshot(files: [], text: text, html: data(.html), rtf: data(.rtf), png: nil, tiff: nil)
        }
        let png = data(.png)
        return Snapshot(files: [], text: nil, html: nil, rtf: nil, png: png, tiff: png == nil ? data(.tiff) : nil)
    }

    /// What the pasteboard holds now, as a clip (reads and converts on the calling thread).
    public static func read(_ pb: NSPasteboard, maxBytes: UInt64) -> ClipRead {
        snapshot(pb).build(maxBytes: maxBytes)
    }

    /// Regular files only; folders are out of scope for the MVP.
    static func fileURLs(_ pb: NSPasteboard) -> [URL] {
        let urls = pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return urls.filter { url in
            var isDir: ObjCBool = false
            return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && !isDir.boolValue
        }
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

// LocalClip is generated (not marked Sendable) but only holds value types (Data, String, arrays).
extension RepresentationMapper.ClipRead: @unchecked Sendable {}
