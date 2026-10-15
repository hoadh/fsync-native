import FsyncCoreKit

extension EngineBridge {
    /// Routes one engine callback. Always on the main actor, in engine order.
    func receive(_ event: EngineEvent) {
        switch event {
        case .account(let account): handle(account)
        case .deviceStatus(let device): devicesModel?.merge(device)
        case .pairing(let pairing): pairingModel?.handle(pairing)
        case .error(let error):
            if ErrorText.requiresSignIn(error) { requireSignIn() } else if let text = ErrorText.message(for: error, context: .general) {
                banner = text
            }
        case .remoteClip(let item):
            clipSync?.enqueue(item)
            refreshHistory()  // the engine recorded the item before this event
        case .transfer(let event): transfers?.handle(event)
        case .incomingFiles(let offer):
            Task { await filesSync?.receive(offer: offer) }
            refreshHistory()
        }
    }

    /// Reloads the history list so an open (or reopened) history window shows new items.
    public func refreshHistory() {
        Task { [weak self] in await self?.historyModel?.refresh() }
    }

    private func handle(_ event: AccountEvent) {
        switch event {
        case .historyReset(let backup):
            historyResetBackup = backup
            banner = "Your clipboard history was reset. The old file was kept as a backup."
        case .sessionExpired: requireSignIn()
        case .selfRemoved:
            // Another device removed this Mac: it must be paired again.
            routeToPairing()
        case .listAdvanced: reloadDevices()
        case .trustReset:
            trustModel?.handle(event)
            windowRequest.send()
        }
    }
}
