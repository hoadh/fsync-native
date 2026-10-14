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
    @Combine.Published public private(set) var activity: SyncActivity?
    /// Manual mode: the newest clip from another device, waiting for the user ("Paste from <device>").
    @Combine.Published public private(set) var pendingIncoming: ClipItem?
    @Combine.Published public private(set) var message: String?

    private static let log = Logger(subsystem: AppConfig.bundleId, category: "clipboard")
    private let engine: FsyncEngineProtocol
    private let pasteboard: NSPasteboard
    private let writer: ClipboardWriter
    private var watcher: PasteboardWatcher!
    private(set) var settings: Settings
    private var filter: SensitiveFilter
    private var running = false
    private var publishing = false

    /// `fetchRep` defaults to the engine's `fetch_rep`.
    public init(engine: FsyncEngineProtocol, pasteboard: NSPasteboard = .general, settings: Settings,
                fetchRep: RepFetcher? = nil,
                frontmost: @escaping () -> String? = { NSWorkspace.shared.frontmostApplication?.bundleIdentifier }) {
        self.engine = engine
        self.pasteboard = pasteboard
        self.settings = settings
        filter = SensitiveFilter(userDenylist: settings.denylist)
        writer = ClipboardWriter(pasteboard: pasteboard, fetch: fetchRep ?? { [engine] id, idx in try engine.fetchRep(itemId: id, idx: idx) })
        watcher = PasteboardWatcher(pasteboard: pasteboard, filter: filter, frontmost: frontmost) { [weak self] change in
            self?.changed(change)
        }
    }

    public var mode: ClipMode { settings.clipMode }
    public var isRunning: Bool { running }

    /// Begins watching (auto mode) and applying incoming clips. Safe to call twice.
    public func start() {
        running = true
        reconfigure()
    }

    public func stop() {
        running = false
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
    public func prepareToQuit() { writer.clearIfOurs() }

    // MARK: Outgoing

    private func changed(_ change: PasteboardChange) {
        guard settings.clipMode == .auto, change.changeCount != writer.lastWriteCount else { return }
        switch change.decision {
        case .sync: Task { await publishCurrent(explicit: false) }
        case .skip(let reason): Self.log.debug("skipped change: \(reason.description, privacy: .public)")
        }
    }

    /// "Send clipboard" (menu / hotkey) in manual mode; also what the watcher calls in auto mode.
    public func publishCurrent(explicit: Bool = true) async {
        guard running, !publishing else { return }
        if explicit {
            let types = (pasteboard.types ?? []).map(\.rawValue)
            if case .skip(let reason) = filter.decide(types: types, frontmostBundleId: nil) {
                message = reason.userText
                return
            }
        }
        publishing = true
        defer { publishing = false }
        switch RepresentationMapper.read(pasteboard, maxBytes: settings.maxClipBytes) {
        case .empty: if explicit { message = "Nothing to send." }
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

    // MARK: Incoming

    private var chain: Task<Void, Never>?

    /// Queues a clip from the engine; clips are applied one at a time, in arrival order.
    public func enqueue(_ item: ClipItem) {
        let previous = chain
        chain = Task { [weak self] in
            await previous?.value
            await self?.receive(item)
        }
    }

    /// A clip from another device. Auto mode writes it; manual mode keeps the newest for "Paste from…".
    public func receive(_ item: ClipItem) async {
        guard running, item.kind != .files else { return }
        if settings.clipMode == .auto { await apply(item) } else { pendingIncoming = item }
    }

    /// Manual mode: the user chose "Paste from <device>".
    public func acceptPending() async {
        guard let item = pendingIncoming else { return }
        pendingIncoming = nil
        await apply(item)
    }

    private func apply(_ item: ClipItem) async {
        switch await writer.apply(item) {
        case .success: activity = SyncActivity(direction: .received, summary: item.preview ?? "Clip", at: Date())
        case .failure(.nothingUsable): Self.log.info("received clip has no usable representation")
        case .failure(.fetchFailed): message = "Couldn't receive the clip from \(PeerText.sanitize(item.fromName))."
        }
    }

    /// History copy-back: only the stored preview (200 characters) is available.
    @discardableResult
    public func copyBack(text: String) -> Bool { writer.writeText(text) }

    public func report(_ text: String) { message = text }

    public func dismissMessage() { message = nil }
}
