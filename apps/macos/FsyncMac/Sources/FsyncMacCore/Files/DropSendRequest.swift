import Foundation

public enum DropError: Error, Equatable, Sendable {
    case empty
    /// A dropped item is not a `file:` URL (a web link, for example).
    case notFileURL
    /// The item no longer exists; carries the file name only.
    case missing(name: String)
}

/// Checks what was dropped on a device before anything is sent.
public enum DropSendRequest {
    /// File URLs only, symlinks resolved, items that do not exist rejected, duplicates dropped (order kept).
    public static func validate(_ urls: [URL]) -> Result<[URL], DropError> {
        var seen = Set<String>()
        var out: [URL] = []
        for url in urls {
            guard url.isFileURL else { return .failure(.notFileURL) }
            let resolved = url.resolvingSymlinksInPath()
            guard FileManager.default.fileExists(atPath: resolved.path) else {
                return .failure(.missing(name: PeerText.sanitize(resolved.lastPathComponent)))
            }
            if seen.insert(resolved.path).inserted { out.append(resolved) }
        }
        return out.isEmpty ? .failure(.empty) : .success(out)
    }
}
