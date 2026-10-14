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

    /// Runs on every successful settings load or edit. The first successful load while `.ready` starts
    /// clipboard sync, even when an earlier load failed, so a transient read error never leaves it off.
    private func clipSettings(changed new: Settings) {
        let retentionChanged = new.retentionDays != clipSync?.retentionDays || new.retentionBytes != clipSync?.retentionBytes
        clipSync?.apply(settings: new)
        guard phase == .ready else { return }
        if clipSync?.isRunning == false {
            startClipboard()
        } else if retentionChanged {
            startInboxCleanup()
        }
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
        Task { [weak self] in await self?.loadClipSettings() }
    }

    /// The stored settings decide the mode, so sync does not start on guesses: when they can't be read the
    /// clipboard stays off and the error shows in Settings. A later successful load starts it
    /// (see `clipSettings(changed:)`).
    func loadClipSettings() async {
        guard let settings = clipSettings else { return }
        if await !settings.load() {
            clipSync?.report("Clipboard sync is off: your settings couldn't be read.")
        }
    }

    private func startClipboard() {
        clipSync?.dismissMessage()
        clipSync?.start()
        if hotkey?.register() == false { clipSync?.report("The shortcut \(HotkeyManager.sendClipboardLabel) is used by another app.") }
        startInboxCleanup()
        Task { [weak self] in await self?.historyModel?.reload() }
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
