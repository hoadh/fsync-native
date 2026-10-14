import Foundation

/// A JSON value, as a genuinely `Sendable` tree.
enum CliJSON: Sendable, Equatable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case null
    case array([CliJSON])
    case object([String: CliJSON])

    init(_ any: Any) {
        switch any {
        case let n as NSNumber where CFGetTypeID(n) == CFBooleanGetTypeID(): self = .bool(n.boolValue)
        case let n as NSNumber: self = .number(n.doubleValue)
        case let s as String: self = .string(s)
        case let a as [Any]: self = .array(a.map(CliJSON.init))
        case let o as [String: Any]: self = .object(o.mapValues(CliJSON.init))
        default: self = .null
        }
    }
}

/// One JSON line printed by `fsync-cli`, read through typed accessors.
struct CliEvent: Sendable, Equatable {
    private let fields: [String: CliJSON]

    init(_ fields: [String: Any]) { self.fields = fields.mapValues(CliJSON.init) }

    func string(_ key: String) -> String? { if case .string(let s)? = fields[key] { return s } else { return nil } }

    func int(_ key: String) -> Int? {
        if case .number(let n)? = fields[key], n == n.rounded(), abs(n) < 9e15 { return Int(n) } else { return nil }
    }

    func bool(_ key: String) -> Bool? { if case .bool(let b)? = fields[key] { return b } else { return nil } }
}
