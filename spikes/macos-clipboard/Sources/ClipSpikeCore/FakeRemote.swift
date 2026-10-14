import Foundation

/// Stand-in for `fetch_file_to`: waits `delay` seconds, then writes `size` deterministic bytes.
public struct FakeRemoteFile {
    public let name: String
    public let size: Int
    public let delay: TimeInterval
    public let fails: Bool

    public init(name: String, size: Int, delay: TimeInterval = 0, fails: Bool = false) {
        self.name = name; self.size = size; self.delay = delay; self.fails = fails
    }

    /// Byte `i` of the file; both sides can recompute the expected hash from `size` alone.
    public static func chunk(offset: Int, count: Int) -> Data {
        Data((0..<count).map { UInt8(truncatingIfNeeded: (offset + $0) &* 31 &+ 7) })
    }

    public func materialize(in dir: URL) throws -> URL {
        if delay > 0 { Thread.sleep(forTimeInterval: delay) }
        if fails { throw CocoaError(.fileReadUnknown) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(name)
        FileManager.default.createFile(atPath: url.path, contents: nil)
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        var written = 0
        let step = 1 << 20
        while written < size {
            let n = min(step, size - written)
            try handle.write(contentsOf: Self.chunk(offset: written, count: n))
            written += n
        }
        return url
    }
}
