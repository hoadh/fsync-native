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

    private let engine: FsyncEngineProtocol
    private let onChange: @MainActor (Settings) -> Void

    /// Loads the stored settings, or the engine's defaults when they can't be read.
    public init(engine: FsyncEngineProtocol, onChange: @escaping @MainActor (Settings) -> Void = { _ in }) {
        self.engine = engine
        self.onChange = onChange
        settings = (try? engine.settings()) ?? Settings(
            clipMode: .auto, retentionDays: 30, retentionBytes: 1 << 30, maxClipBytes: 50 << 20, denylist: [], autoAcceptFiles: false)
    }

    /// Applies `edit` to a copy, clamps it to the core's limits and saves it. Reverts on failure.
    public func update(_ edit: (inout Settings) -> Void) async {
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
