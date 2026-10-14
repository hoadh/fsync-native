import AppKit
import Combine
import FsyncCoreKit
import Foundation
import os

public struct SyncActivity: Equatable, Sendable {
    public enum Direction: Sendable { case sent, received }
    public let direction: Direction
    public let summary: String
    public let at: Date
}

/// Clipboard sync for this Mac: watches the pasteboard and publishes (auto mode, or on request), and
/// applies clips from other devices. All engine calls go through `EngineCall` (never on the main actor).
@MainActor
public final class ClipboardSync: ObservableObject {
    @Combine.Published public internal(set) var activity: SyncActivity?
    /// Manual mode: the newest clip from another device, waiting for the user ("Paste from <device>").
    @Combine.Published public internal(set) var pendingIncoming: ClipItem?
    @Combine.Published public internal(set) var message: String?

    static let log = Logger(subsystem: AppConfig.bundleId, category: "clipboard")
    private let engine: FsyncEngineProtocol
    private let pasteboard: NSPasteboard
    let writer: ClipboardWriter
    let fileWriter: FileClipWriter?
    private var watcher: PasteboardWatcher!
    private(set) var settings: Settings
    private var filter: SensitiveFilter
    var running = false
    private var publishing = false
    private var queued: Request?
    var chain: Task<Void, Never>?

    /// `fetchRep` defaults to the engine's `fetch_rep`.
    public init(engine: FsyncEngineProtocol, pasteboard: NSPasteboard = .general, settings: Settings,
                fetchRep: RepFetcher? = nil, fileWriter: FileClipWriter? = nil,
                frontmost: @escaping () -> String? = { NSWorkspace.shared.frontmostApplication?.bundleIdentifier }) {
        self.engine = engine
        self.pasteboard = pasteboard
        self.settings = settings
        self.fileWriter = fileWriter
        filter = SensitiveFilter(userDenylist: settings.denylist)
        writer = ClipboardWriter(pasteboard: pasteboard, fetch: fetchRep ?? { [engine] id, idx in try engine.fetchRep(itemId: id, idx: idx) })
        watcher = PasteboardWatcher(pasteboard: pasteboard, filter: filter, frontmost: frontmost) { [weak self] change in
            self?.changed(change)
        }
    }

    public var mode: ClipMode { settings.clipMode }
    public var isRunning: Bool { running }
    public var retentionDays: UInt32 { settings.retentionDays }
    public var retentionBytes: UInt64 { settings.retentionBytes }

    /// Begins watching (auto mode) and applying incoming clips. Safe to call twice.
    public func start() {
        running = true
        reconfigure()
    }

    public func stop() {
        running = false
        queued = nil
        watcher.stop()
        pendingIncoming = nil
    }

    /// Applies new settings: polling only runs in auto mode; clips already on the pasteboard are not sent.
    public func apply(settings new: Settings) {
        let modeChanged = new.clipMode != settings.clipMode
        settings = new
        filter = SensitiveFilter(userDenylist: new.denylist)
        watcher.update(filter: filter)
        if modeChanged { reconfigure() }
        if new.clipMode == .auto { pendingIncoming = nil }
    }

    private func reconfigure() {
        guard running, settings.clipMode == .auto else { return watcher.stop() }
        if !watcher.isRunning { watcher.start() }
    }

    /// App is quitting: do not leave an entry we can no longer serve.
    public func prepareToQuit() {
        writer.clearIfOurs()
        fileWriter?.clearIfOurs()
    }

    // MARK: Outgoing

    /// A request to publish. `expected` (auto mode) is the changeCount the privacy filter decided on.
    struct Request {
        let explicit: Bool
        let expected: Int?
    }

    private func changed(_ change: PasteboardChange) {
        guard settings.clipMode == .auto, change.changeCount != writer.lastWriteCount,
              change.changeCount != fileWriter?.lastWriteCount else { return }
        switch change.decision {
        case .sync: Task { await publish(Request(explicit: false, expected: change.changeCount)) }
        case .skip(let reason): Self.log.debug("skipped change: \(reason.description, privacy: .private)")
        }
    }

    /// "Send clipboard" (menu / hotkey) in manual mode.
    public func publishCurrent(explicit: Bool = true) async {
        await publish(Request(explicit: explicit, expected: nil))
    }

    /// One publish at a time. A request that arrives meanwhile is kept (the newest wins) and runs after
    /// the current one, so a quick second copy is not lost.
    func publish(_ request: Request) async {
        guard running else { return }
        if publishing { queued = request; return }
        publishing = true
        var next: Request? = request
        while let current = next {
            queued = nil
            await publishOnce(current)
            next = running ? queued : nil
        }
        queued = nil
        publishing = false
    }

    /// Reads the pasteboard only if it is still the change the filter looked at, and re-checks before sending.
    private func publishOnce(_ request: Request) async {
        let count = pasteboard.changeCount
        if let expected = request.expected, expected != count { return }
        if request.explicit {
            let types = (pasteboard.types ?? []).map(\.rawValue)
            if case .skip(let reason) = filter.decide(types: types, frontmostBundleId: nil) {
                message = reason.userText
                return
            }
        }
        let snapshot = RepresentationMapper.snapshot(pasteboard)
        guard pasteboard.changeCount == count else { return }
        let limit = settings.maxClipBytes
        let read = await Task.detached(priority: .userInitiated) { snapshot.build(maxBytes: limit) }.value
        guard running, pasteboard.changeCount == count else { return }
        switch read {
        case .empty: if request.explicit { message = "Nothing to send." }
        case .tooLarge: message = ErrorText.message(for: FsyncError.TooLarge, context: .general)
        case .clip(let clip): await send(clip)
        }
    }

    private func send(_ clip: LocalClip) async {
        let result = await EngineCall.run(engine) { try $0.publishClip(clip: clip) }
        switch result {
        case .failure(let error): message = ErrorText.message(for: error, context: .general)
        case .success(let published):
            if published.echo { return }
            message = published.failed.isEmpty ? nil : "Some devices couldn't be reached."
            activity = SyncActivity(direction: .sent, summary: clip.preview ?? "Clip", at: Date())
        }
    }

    public func report(_ text: String) { message = text }

    public func dismissMessage() { message = nil }
}
