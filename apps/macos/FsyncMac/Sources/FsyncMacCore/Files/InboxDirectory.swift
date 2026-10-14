import Foundation

/// Where files of received clips are downloaded: `~/Library/Caches/fsync/inbox/<item id>/`, user-only.
public struct InboxDirectory: Sendable {
    public let root: URL

    public init(root: URL) { self.root = root }

    public static func standard() -> InboxDirectory {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return InboxDirectory(root: caches.appendingPathComponent("fsync/inbox", isDirectory: true))
    }

    /// Folder for one item, created with 0700. Item ids come from other devices, so only a plain
    /// hex/dash id is accepted as a folder name.
    public func folder(forItem itemId: String) throws -> URL {
        guard !itemId.isEmpty, itemId.count <= 64,
              itemId.allSatisfy({ $0.isASCII && ($0.isHexDigit || $0 == "-") }) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        let dir = root.appendingPathComponent(itemId, isDirectory: true)
        try FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        return dir
    }

    /// Deletes item folders last modified before `cutoff`, except the ones in `keeping`. Returns the count.
    @discardableResult
    public func removeOld(before cutoff: Date, keeping: Set<String>) -> Int {
        let fm = FileManager.default
        let entries = (try? fm.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.contentModificationDateKey], options: [.skipsHiddenFiles])) ?? []
        var removed = 0
        for url in entries where !keeping.contains(url.lastPathComponent) {
            let modified = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
            guard let modified, modified < cutoff else { continue }
            if (try? fm.removeItem(at: url)) != nil { removed += 1 }
        }
        return removed
    }
}
