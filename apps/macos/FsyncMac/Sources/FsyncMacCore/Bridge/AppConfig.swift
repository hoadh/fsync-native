import Foundation

/// Where the engine talks to and what this Mac is called.
public struct AppConfig: Sendable, Equatable {
    public static let bundleId = "app.fsync.mac"
    public static let defaultApiBase = "http://127.0.0.1:8080"

    public var apiBase: String
    public var relayUrl: String?
    public var deviceName: String

    public init(apiBase: String, relayUrl: String?, deviceName: String) {
        self.apiBase = apiBase
        self.relayUrl = relayUrl
        self.deviceName = deviceName
    }

    /// Plain http is only allowed for a dev backend on this machine.
    public var allowInsecureHttp: Bool {
        guard let url = URL(string: apiBase), url.scheme == "http" else { return false }
        return url.host == "127.0.0.1" || url.host == "localhost"
    }

    /// Env vars `FSYNC_API` / `FSYNC_RELAY` (dev) override Info.plist `FsyncApiBase` / `FsyncRelayUrl`.
    /// Empty values count as unset.
    public static func resolve(env: [String: String], info: [String: Any]?, deviceName: String) -> AppConfig {
        func pick(_ envKey: String, _ infoKey: String) -> String? {
            for value in [env[envKey], info?[infoKey] as? String] {
                if let v = value?.trimmingCharacters(in: .whitespaces), !v.isEmpty { return v }
            }
            return nil
        }
        return AppConfig(
            apiBase: pick("FSYNC_API", "FsyncApiBase") ?? defaultApiBase,
            relayUrl: pick("FSYNC_RELAY", "FsyncRelayUrl"),
            deviceName: deviceName)
    }

    /// Config for the running app.
    public static func live() -> AppConfig {
        resolve(env: ProcessInfo.processInfo.environment, info: Bundle.main.infoDictionary,
                deviceName: Host.current().localizedName ?? "Mac")
    }

    /// `~/Library/Application Support/app.fsync.mac/`, created with mode 0700.
    public static func defaultDataDir() throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let dir = base.appendingPathComponent(bundleId, isDirectory: true)
        try Self.createPrivateDir(dir)
        return dir
    }

    /// Used when the Application Support folder is unusable: a per-user 0700 folder in the temp dir.
    public static func fallbackDataDir() -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(bundleId, isDirectory: true)
        // Best effort: if even this fails the engine reports the real error at boot.
        try? createPrivateDir(dir)
        return dir
    }

    /// Creates `dir` with mode 0700 and tightens an existing one.
    static func createPrivateDir(_ dir: URL) throws {
        let fm = FileManager.default
        try fm.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path)
    }
}
