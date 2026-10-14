import FsyncCoreKit
import Foundation

extension EngineBridge {
    private static let inboxCleanupInterval: UInt64 = 24 * 3600 * 1_000_000_000

    /// Clipboard sync, files, settings and the history list share the engine; they run only while `.ready`.
    /// Must run after `modelSubscriptions` was reset.
    func installClipboardModels(engine: FsyncEngine) {
        let settings = ClipSettingsModel(engine: engine) { [weak self] in self?.clipSettings(changed: $0) }
        let transfers = TransfersModel(engine: engine)
        let fetcher = FileFetchService(engine: engine, transfers: transfers)
        let writer = FileClipWriter(pasteboard: pasteboard, fetcher: fetcher, inbox: inbox)
        let sync = ClipboardSync(engine: engine, pasteboard: pasteboard, settings: settings.settings, fileWriter: writer)
        let files = FilesSync(engine: engine, fetcher: fetcher, downloads: downloads) { [weak settings] in settings?.settings.autoAcceptFiles ?? false }
        (clipSettings, clipSync, fileWriter) = (settings, sync, writer)
        (self.transfers, filesSync) = (transfers, files)
        historyModel = HistoryModel(engine: engine)
        hotkey?.unregister()
        hotkey = HotkeyManager { [weak sync] in Task { await sync?.publishCurrent() } }
        $phase.sink { [weak self] phase in
            MainActor.assumeIsolated { self?.phaseChanged(for: phase) }
        }.store(in: &modelSubscriptions)
    }

    private func clipSettings(changed new: Settings) {
        let retentionChanged = new.retentionDays != clipSync?.retentionDays || new.retentionBytes != clipSync?.retentionBytes
        clipSync?.apply(settings: new)
        if retentionChanged, phase == .ready { startInboxCleanup() }
    }

    private func phaseChanged(for phase: AppPhase) {
        guard phase == .ready else {
            clipSync?.stop()
            filesSync?.stop()
            hotkey?.unregister()
            inboxCleanup?.cancel()
            return
        }
        filesSync?.start()
        Task { [weak self] in await self?.startClipboardWhenSettingsAreKnown() }
    }

    /// The stored settings decide the mode, so sync does not start on guesses: when they can't be read the
    /// clipboard stays off and the error shows in Settings.
    private func startClipboardWhenSettingsAreKnown() async {
        guard let settings = clipSettings else { return }
        guard await settings.load() else {
            clipSync?.report("Clipboard sync is off: your settings couldn't be read.")
            return
        }
        guard phase == .ready else { return }
        clipSync?.apply(settings: settings.settings)
        clipSync?.start()
        if hotkey?.register() == false { clipSync?.report("The shortcut \(HotkeyManager.sendClipboardLabel) is used by another app.") }
        await historyModel?.reload()
        startInboxCleanup()
    }

    /// Downloaded clip files follow the history retention (days and size); runs now and then daily.
    private func startInboxCleanup() {
        inboxCleanup?.cancel()
        inboxCleanup = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, let settings = self.clipSettings?.settings else { return }
                let (inbox, keep) = (self.inbox, self.fileWriter?.currentItemId)
                await Task.detached(priority: .utility) {
                    inbox.clean(retentionDays: settings.retentionDays, maxBytes: settings.retentionBytes, keeping: keep.map { [$0] } ?? [])
                }.value
                try? await Task.sleep(nanoseconds: Self.inboxCleanupInterval)
            }
        }
    }

    /// The app is quitting: do not leave a pasteboard entry that nothing can serve any more.
    public func prepareToQuit() {
        clipSync?.prepareToQuit()
        hotkey?.unregister()
    }

    func removeClipboardModels() {
        inboxCleanup?.cancel()
        inboxCleanup = nil
        clipSync?.stop()
        prepareToQuit()
        filesSync?.stop()
        (hotkey, clipSync, clipSettings, historyModel, filesSync, transfers, fileWriter) = (nil, nil, nil, nil, nil, nil, nil)
    }
}
