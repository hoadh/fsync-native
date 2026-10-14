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

    /// Keeps the inbox within `maxBytes` by deleting the oldest item folders first (never `keeping`).
    @discardableResult
    public func trim(toBytes maxBytes: UInt64, keeping: Set<String>) -> Int {
        let fm = FileManager.default
        let keys: [URLResourceKey] = [.contentModificationDateKey]
        let entries = (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles])) ?? []
        var folders: [(url: URL, modified: Date, size: UInt64)] = entries.map {
            ($0, (try? $0.resourceValues(forKeys: Set(keys)))?.contentModificationDate ?? .distantPast, Self.size(of: $0))
        }
        var total = folders.reduce(0) { $0 + $1.size }
        folders.sort { $0.modified < $1.modified }
        var removed = 0
        for folder in folders where total > maxBytes && !keeping.contains(folder.url.lastPathComponent) {
            if (try? fm.removeItem(at: folder.url)) != nil { total -= folder.size; removed += 1 }
        }
        return removed
    }

    /// Retention for downloaded clip files: older than `days` goes, then the oldest until under `maxBytes`.
    public func clean(retentionDays days: UInt32, maxBytes: UInt64, keeping: Set<String>, now: Date = Date()) {
        removeOld(before: now.addingTimeInterval(-Double(days) * 86_400), keeping: keeping)
        trim(toBytes: maxBytes, keeping: keeping)
    }

    private static func size(of folder: URL) -> UInt64 {
        let files = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: [.totalFileAllocatedSizeKey])
        var total: UInt64 = 0
        while let url = files?.nextObject() as? URL {
            total += UInt64((try? url.resourceValues(forKeys: [.totalFileAllocatedSizeKey]))?.totalFileAllocatedSize ?? 0)
        }
        return total
    }
}
