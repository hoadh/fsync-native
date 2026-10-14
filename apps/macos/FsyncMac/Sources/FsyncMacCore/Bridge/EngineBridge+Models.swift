import AppKit
import Combine
import FsyncCoreKit
import TrustResetPrompt

extension EngineBridge {
    /// Creates the models that need a running engine and wires their results back to the bridge.
    func installAccountModels(engine: FsyncEngine) {
        let signIn: @MainActor () -> Void = { [weak self] in self?.requireSignIn(banner: "Sign in again to continue.") }
        let devices = DevicesModel(engine: engine, onSignInRequired: signIn)
        devicesModel = devices
        pairingModel = PairingModel(engine: engine, onPaired: { [weak self] in self?.afterPaired(peer: $0) },
                                    onSignInRequired: signIn)
        let trust = TrustResetPromptModel(engine: engine)
        trustModel = trust

        modelSubscriptions = []
        installClipboardModels(engine: engine)
        devices.$devices.sink { [weak self] list in
            MainActor.assumeIsolated { self?.anyPeerOnline = StatusSummary(list).online > 0 }
        }.store(in: &modelSubscriptions)
        // A trusted list without this Mac means it must be paired again.
        trust.$outcome.sink { [weak self] outcome in
            MainActor.assumeIsolated { self?.handleTrustOutcome(outcome) }
        }.store(in: &modelSubscriptions)
        // Status events can be missed while the Mac sleeps.
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didWakeNotification)
            .sink { [weak self] _ in MainActor.assumeIsolated { self?.reloadDevices() } }
            .store(in: &modelSubscriptions)
    }

    func handleTrustOutcome(_ outcome: TrustResetOutcome) {
        guard outcome == .removedNeedsPairing else { return }
        routeToPairing()
    }

    func resetAccountModels() {
        devicesModel?.clear()
        clipSync?.stop()
        filesSync?.stop()
        transfers?.reset()
        historyModel?.reset()
        trustModel?.dismissOutcome()
        Task { [pairingModel] in await pairingModel?.cancel() }
    }

    /// Fire-and-forget refresh of the device list.
    public func reloadDevices() {
        guard phase == .ready, let devicesModel else { return }
        Task { await devicesModel.reload() }
    }

    /// Pairing finished on either side. The offering Mac is now trusted, so ask the server again.
    func afterPaired(peer: String) {
        if phase == .needsPairing {
            bootTask?.cancel()
            bootTask = Task { [weak self] in
                await self?.resolveAccount()
                if self?.phase == .ready { self?.banner = "Paired with \(peer)." }
            }
        } else {
            banner = "Paired with \(peer)."
            reloadDevices()
        }
    }
}
