import Foundation
import Security

/// A failed Keychain call; `status` is the `OSStatus`.
public struct KeychainError: Error, Equatable, Sendable {
    public let status: OSStatus
}

/// Generic-password items for one service.
///
/// Store choice (probed once per process, see `useDataProtection`): the data-protection Keychain
/// needs an entitlement that an unsigned or ad-hoc signed binary cannot carry (`errSecMissingEntitlement`,
/// -34018; confirmed with swiftc + `codesign -s -`). Then the file-based login keychain is used. Both
/// stores get `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, which the file keychain accepts but
/// does not enforce.
///
/// Upgrade path: a build that was unsigned stored its items in the file keychain. When a later signed
/// build can use the data-protection keychain, a `get` miss there falls back to the file keychain and
/// moves the item over (`readMigrating`), so `device_secret`, `history_key` and the session survive.
/// `delete` removes both copies, so a signed-out session cannot come back from the old store.
///
/// All calls in the process take `lock`: the file keychain can deadlock inside `SecItemAdd` when two
/// threads add the same item at once (seen in the concurrent-first-write test), and the engine's
/// session callback does race the login model.
public struct KeychainStore: Sendable {
    public let service: String
    private static let lock = NSRecursiveLock()

    public init(service: String = AppConfig.bundleId) {
        self.service = service
        self.forceFile = false
    }

    /// True when the data-protection Keychain accepts writes from this process.
    public static let useDataProtection: Bool = {
        let probe = KeychainStore(service: "\(AppConfig.bundleId).probe", forceFile: true)
        var query = probe.base("probe", dataProtection: true)
        query[kSecValueData as String] = Data([0])
        let status = SecItemAdd(query as CFDictionary, nil)
        let supported = Self.isSupported(addStatus: status)
        if supported { SecItemDelete(probe.base("probe", dataProtection: true) as CFDictionary) }
        return supported
    }()

    /// A leftover probe item (`errSecDuplicateItem`) proves the store works just as well as a fresh add.
    static func isSupported(addStatus: OSStatus) -> Bool {
        addStatus == errSecSuccess || addStatus == errSecDuplicateItem
    }

    private let forceFile: Bool

    private init(service: String, forceFile: Bool) {
        self.service = service
        self.forceFile = forceFile
    }

    private func base(_ account: String, dataProtection: Bool) -> [String: Any] {
        var q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        if dataProtection { q[kSecUseDataProtectionKeychain as String] = true }
        return q
    }

    private var usesDataProtection: Bool { !forceFile && Self.useDataProtection }

    private func query(_ account: String) -> [String: Any] {
        base(account, dataProtection: usesDataProtection)
    }

    /// The stored bytes, or nil when there is no item. In data-protection mode a miss falls back to
    /// the file keychain and moves the item over.
    public func get(_ account: String) throws -> Data? {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        guard usesDataProtection else { return try read(base(account, dataProtection: false)) }
        return try Self.readMigrating(
            primary: { try read(query(account)) },
            legacy: { try read(base(account, dataProtection: false)) },
            store: { try set($0, account: account) },
            dropLegacy: { try remove(base(account, dataProtection: false)) })
    }

    /// Reads `primary`; on a miss reads `legacy` and, if found, stores it in the primary store and
    /// drops the legacy copy. A failed copy still returns the data and keeps the legacy item.
    static func readMigrating(
        primary: () throws -> Data?, legacy: () throws -> Data?,
        store: (Data) throws -> Void, dropLegacy: () throws -> Void
    ) throws -> Data? {
        if let data = try primary() { return data }
        guard let data = try legacy() else { return nil }
        do {
            try store(data)
            try dropLegacy()
        } catch {
            // The data is still good; the move is retried on the next read.
        }
        return data
    }

    private func read(_ base: [String: Any]) throws -> Data? {
        var q = base
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &out)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = out as? Data else { throw KeychainError(status: status) }
        return data
    }

    /// Adds the item. False when one already exists (it is left untouched).
    public func add(_ data: Data, account: String) throws -> Bool {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        var add = query(account)
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        if status == errSecDuplicateItem { return false }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
        return true
    }

    /// Adds or replaces the item.
    public func set(_ data: Data, account: String) throws {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        let update = SecItemUpdate(query(account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw KeychainError(status: update) }
        if try add(data, account: account) { return }
        // Another writer (the engine's session callback races the login model) added it first.
        let status = SecItemUpdate(query(account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    /// Removes the item (and any copy left in the file keychain); a missing item is not an error.
    public func delete(_ account: String) throws {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        try remove(query(account))
        if usesDataProtection { try remove(base(account, dataProtection: false)) }
    }

    private func remove(_ query: [String: Any]) throws {
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw KeychainError(status: status) }
    }
}
