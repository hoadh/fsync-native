import Combine
import FsyncCoreKit
import Foundation

/// Sync settings stored by the engine (mode, retention, max clip size, denylist, file auto-accept).
@MainActor
public final class ClipSettingsModel: ObservableObject {
    public static let retentionDaysRange: ClosedRange<UInt32> = 1...3650
    public static let minBytes: UInt64 = 1 << 20

    @Combine.Published public private(set) var settings: Settings
    @Combine.Published public private(set) var errorMessage: String?

    /// False until the stored settings were read; edits are refused before (they would overwrite them).
    @Combine.Published public private(set) var isLoaded = false

    private let engine: FsyncEngineProtocol
    private let onChange: @MainActor (Settings) -> Void
    /// Test seam: how `load()` reads the stored settings.
    var readSettings: @Sendable (FsyncEngineProtocol) throws -> Settings = { try $0.settings() }

    /// Starts with the engine's defaults as placeholders; call `load()` to read the stored settings.
    public init(engine: FsyncEngineProtocol, onChange: @escaping @MainActor (Settings) -> Void = { _ in }) {
        self.engine = engine
        self.onChange = onChange
        settings = Settings(
            clipMode: .auto, retentionDays: 30, retentionBytes: 1 << 30, maxClipBytes: 50 << 20, denylist: [], autoAcceptFiles: false)
    }

    /// Reads the stored settings off the main actor. On failure the error is shown and edits stay refused.
    @discardableResult
    public func load() async -> Bool {
        let read = readSettings
        let result = await EngineCall.run(engine) { try read($0) }
        switch result {
        case .success(let stored):
            settings = stored
            isLoaded = true
            errorMessage = nil
            onChange(stored)
            return true
        case .failure(let error):
            errorMessage = ErrorText.message(for: error, context: .general) ?? "Couldn't read your settings."
            return false
        }
    }

    /// Applies `edit` to a copy, clamps it to the core's limits and saves it. Reverts on failure.
    public func update(_ edit: (inout Settings) -> Void) async {
        guard isLoaded else {
            if await !load() { return }
            return await update(edit)
        }
        var next = settings
        edit(&next)
        next.retentionDays = min(max(next.retentionDays, Self.retentionDaysRange.lowerBound), Self.retentionDaysRange.upperBound)
        next.retentionBytes = max(next.retentionBytes, Self.minBytes)
        next.maxClipBytes = max(next.maxClipBytes, Self.minBytes)
        next.denylist = Self.cleaned(next.denylist)
        guard next != settings else { return }
        let value = next
        let result = await EngineCall.run(engine) { try $0.setSettings(settings: value) }
        switch result {
        case .success:
            errorMessage = nil
            settings = value
            onChange(value)
        case .failure(let error):
            errorMessage = ErrorText.message(for: error, context: .general)
        }
    }

    public func addToDenylist(_ bundleId: String) async { await update { $0.denylist.append(bundleId) } }

    public func removeFromDenylist(_ bundleId: String) async { await update { $0.denylist.removeAll { $0 == bundleId } } }

    public func dismissError() { errorMessage = nil }

    /// Trimmed, non-empty, unique, in first-seen order.
    static func cleaned(_ list: [String]) -> [String] {
        var seen = Set<String>()
        return list.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && seen.insert($0).inserted }
    }
}
