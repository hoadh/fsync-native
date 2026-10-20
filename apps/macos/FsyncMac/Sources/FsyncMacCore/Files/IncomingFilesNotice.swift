import Foundation

/// What the user is told when other devices offer files. Built from engine data only: the sender is the
/// sanitized device name, and file names never appear (they are peer-supplied text).
public struct IncomingFilesNotice: Equatable, Sendable {
    public var fromName: String
    public var fileCount: Int
    public var totalBytes: UInt64
    public var autoAccepted: Bool
    /// Offers folded into this notice (1 unless the coalescer merged several).
    public var offerCount: Int

    public init(fromName: String, fileCount: Int, totalBytes: UInt64, autoAccepted: Bool, offerCount: Int = 1) {
        let clean = PeerText.sanitize(fromName)
        self.fromName = clean.isEmpty ? "another device" : clean
        self.fileCount = fileCount
        self.totalBytes = totalBytes
        self.autoAccepted = autoAccepted
        self.offerCount = offerCount
    }

    public var title: String { "Files from \(fromName)" }

    public var body: String {
        if offerCount > 1 { return "\(offerCount) offers from \(fromName)" }
        let files = fileCount == 1 ? "1 file" : "\(fileCount) files"
        let size = ByteCountFormatter.string(fromByteCount: Int64(clamping: totalBytes), countStyle: .file)
        return "\(files), \(size). " + (autoAccepted ? "Saving to Downloads" : "Open fsync to accept")
    }
}
