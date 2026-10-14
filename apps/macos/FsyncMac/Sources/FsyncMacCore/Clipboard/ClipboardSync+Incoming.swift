import AppKit
import FsyncCoreKit
import Foundation
import os

extension ClipboardSync {
    /// Runs `work` after everything queued before it; incoming clips, "Paste from" and copy-back never overlap.
    @discardableResult
    func serialized(_ work: @escaping @MainActor () async -> Void) -> Task<Void, Never> {
        let previous = chain
        let task = Task { await previous?.value; await work() }
        chain = task
        return task
    }

    /// Queues a clip from the engine; clips are applied one at a time, in arrival order.
    public func enqueue(_ item: ClipItem) {
        serialized { [weak self] in await self?.receive(item) }
    }

    /// A clip from another device. Auto mode writes it; manual mode keeps the newest for "Paste from…".
    public func receive(_ item: ClipItem) async {
        guard running, item.kind != .files || fileWriter != nil else { return }
        if settings.clipMode == .auto { await apply(item) } else { pendingIncoming = item }
    }

    /// Manual mode: the user chose "Paste from <device>".
    public func acceptPending() async {
        await serialized { [weak self] in
            guard let self, let item = pendingIncoming else { return }
            pendingIncoming = nil
            await apply(item)
        }.value
    }

    private func apply(_ item: ClipItem) async {
        let result = if item.kind == .files, let fileWriter { await fileWriter.apply(item) } else { await writer.apply(item) }
        switch result {
        case .success: activity = SyncActivity(direction: .received, summary: item.preview ?? "Clip", at: Date())
        case .failure(.nothingUsable): Self.log.info("received clip has no usable representation")
        case .failure(.fetchFailed): message = "Couldn't receive the clip from \(PeerText.sanitize(item.fromName))."
        }
    }

    /// History copy-back: reads the item's full content from the engine (off the main actor) and puts it back
    /// on the pasteboard. Files-only items have no content to restore. Failures set `message`.
    @discardableResult
    public func copyBack(itemId: String) async -> Bool {
        let load = historyReps
        let loaded = await EngineCall.run(engine) { try load($0, itemId) }
        nonisolated(unsafe) var written = false
        await serialized { [weak self] in
            guard let self else { return }
            switch loaded.flatMap({ reps in writer.writeReps(reps).mapError { $0 as Error } }) {
            case .success: written = true
            case .failure(let error):
                Self.log.info("history copy-back failed: \(String(describing: error), privacy: .private)")
                message = "Couldn't copy that clip again."
            }
        }.value
        return written
    }
}
