import Foundation

/// How files that arrive as a clip become pasteable (ADR 009).
public enum FileClipStrategy: String, CaseIterable, Sendable {
    /// Every file is fetched when a reader asks for it.
    case lazy
    /// Files below `lazyThreshold` are fetched before the clip is written, larger ones lazily.
    case hybrid
    /// Every file is fetched before the clip is written (up to `eagerCap`).
    case eager

    public static let lazyThreshold: UInt64 = 20 << 20
    public static let eagerCap: UInt64 = 512 << 20
    public static let defaultsKey = "fileClipStrategy"

    public static func stored(_ defaults: UserDefaults = .standard) -> FileClipStrategy {
        defaults.string(forKey: defaultsKey).flatMap(FileClipStrategy.init(rawValue:)) ?? .hybrid
    }

    public func store(_ defaults: UserDefaults = .standard) { defaults.set(rawValue, forKey: Self.defaultsKey) }

    /// For each size, whether the file is fetched before the write. The eager bytes of one clip never
    /// exceed `eagerCap`; what does not fit is served lazily.
    public func eagerFlags(sizes: [UInt64]) -> [Bool] {
        var eagerBytes: UInt64 = 0
        return sizes.map { size in
            let wanted: Bool
            switch self {
            case .lazy: wanted = false
            case .hybrid: wanted = size < Self.lazyThreshold
            case .eager: wanted = true
            }
            guard wanted, eagerBytes + size <= Self.eagerCap else { return false }
            eagerBytes += size
            return true
        }
    }
}
