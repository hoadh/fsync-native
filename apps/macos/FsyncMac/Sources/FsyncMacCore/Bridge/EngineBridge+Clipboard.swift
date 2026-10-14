import FsyncCoreKit

extension EngineBridge {
    /// Clipboard sync, its settings and the history list share the engine; they run only while `.ready`.
    /// Must run after `modelSubscriptions` was reset.
    func installClipboardModels(engine: FsyncEngine) {
        let settings = ClipSettingsModel(engine: engine) { [weak self] in self?.clipSync?.apply(settings: $0) }
        let sync = ClipboardSync(engine: engine, pasteboard: pasteboard, settings: settings.settings)
        clipSettings = settings
        clipSync = sync
        historyModel = HistoryModel(engine: engine)
        hotkey = HotkeyManager { [weak sync] in Task { await sync?.publishCurrent() } }
        $phase.sink { [weak self] phase in
            MainActor.assumeIsolated { self?.phaseChanged(for: phase) }
        }.store(in: &modelSubscriptions)
    }

    private func phaseChanged(for phase: AppPhase) {
        if phase == .ready {
            clipSync?.start()
            if hotkey?.register() == false { clipSync?.report("The shortcut \(HotkeyManager.sendClipboardLabel) is used by another app.") }
            Task { [historyModel] in await historyModel?.reload() }
        } else {
            clipSync?.stop()
            hotkey?.unregister()
        }
    }

    /// The app is quitting: do not leave a pasteboard entry that nothing can serve any more.
    public func prepareToQuit() { clipSync?.prepareToQuit() }

    func removeClipboardModels() {
        clipSync?.stop()
        clipSync?.prepareToQuit()
        hotkey?.unregister()
        hotkey = nil
        clipSync = nil
        clipSettings = nil
        historyModel = nil
    }
}
