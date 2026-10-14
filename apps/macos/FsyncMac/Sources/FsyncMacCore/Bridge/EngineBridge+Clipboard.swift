import FsyncCoreKit
import Foundation

extension EngineBridge {
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
        hotkey = HotkeyManager { [weak sync] in Task { await sync?.publishCurrent() } }
        $phase.sink { [weak self] phase in
            MainActor.assumeIsolated { self?.phaseChanged(for: phase) }
        }.store(in: &modelSubscriptions)
    }

    private func clipSettings(changed new: Settings) {
        clipSync?.apply(settings: new)
    }

    private func phaseChanged(for phase: AppPhase) {
        if phase == .ready {
            clipSync?.start()
            filesSync?.start()
            if hotkey?.register() == false { clipSync?.report("The shortcut \(HotkeyManager.sendClipboardLabel) is used by another app.") }
            Task { [historyModel] in await historyModel?.reload() }
            cleanInbox()
        } else {
            clipSync?.stop()
            filesSync?.stop()
            hotkey?.unregister()
        }
    }

    /// Downloaded clip files older than the history retention go away; the clip on the pasteboard stays.
    private func cleanInbox() {
        let days = Double(clipSettings?.settings.retentionDays ?? 30)
        let (inbox, keep) = (inbox, fileWriter?.currentItemId)
        Task.detached {
            inbox.removeOld(before: Date().addingTimeInterval(-days * 86_400), keeping: keep.map { [$0] } ?? [])
        }
    }

    /// The app is quitting: do not leave a pasteboard entry that nothing can serve any more.
    public func prepareToQuit() { clipSync?.prepareToQuit() }

    func removeClipboardModels() {
        clipSync?.stop()
        clipSync?.prepareToQuit()
        filesSync?.stop()
        hotkey?.unregister()
        (hotkey, clipSync, clipSettings, historyModel, filesSync, transfers, fileWriter) = (nil, nil, nil, nil, nil, nil, nil)
    }
}
