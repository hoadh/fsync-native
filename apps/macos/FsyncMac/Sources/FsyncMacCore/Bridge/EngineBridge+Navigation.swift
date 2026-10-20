import Combine

extension EngineBridge {
    /// Showing the history page reloads it: the one window stays alive, so its list can be stale.
    /// Fires on every assignment of `.history`, including a repeated `show(.history)`. Must run after
    /// `modelSubscriptions` was reset.
    func installNavigationSinks() {
        navigation.$section.sink { [weak self] section in
            MainActor.assumeIsolated { if section == .history { self?.refreshHistory() } }
        }.store(in: &modelSubscriptions)
    }
}
