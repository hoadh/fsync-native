import FsyncCoreKit
import os

/// Every engine callback, as one value.
public enum EngineEvent: Sendable {
    case remoteClip(ClipItem)
    case deviceStatus(Device)
    case transfer(TransferEvent)
    case incomingFiles(ClipItem)
    case pairing(PairingEvent)
    case account(AccountEvent)
    case error(FsyncError)
}

/// Whether refreshed tokens may be stored. Closed while signed out, so a refresh callback that was
/// already queued in the core when the user signed out cannot bring the session back.
final class SessionGate: Sendable {
    private let state = OSAllocatedUnfairLock(initialState: false)
    var isOpen: Bool { state.withLock { $0 } }
    func set(open: Bool) { state.withLock { $0 = open } }
}

/// Receives callbacks on the engine's own thread. Refreshed tokens are stored right here, so a crash
/// after a refresh cannot lose them; everything else goes to `forward`, which hops to the main actor.
final class EngineDelegateAdapter: EngineDelegate, Sendable {
    private static let log = Logger(subsystem: AppConfig.bundleId, category: "session")
    private let secrets: AppSecrets
    private let gate: SessionGate
    private let forward: @Sendable (EngineEvent) -> Void

    init(secrets: AppSecrets, gate: SessionGate = SessionGate(), forward: @escaping @Sendable (EngineEvent) -> Void) {
        self.secrets = secrets
        self.gate = gate
        self.forward = forward
    }

    func onRemoteClip(item: ClipItem) { forward(.remoteClip(item)) }
    func onDeviceStatus(device: Device) { forward(.deviceStatus(device)) }
    func onTransfer(event: TransferEvent) { forward(.transfer(event)) }
    func onIncomingFiles(offer: ClipItem) { forward(.incomingFiles(offer)) }
    func onPairingEvent(event: PairingEvent) { forward(.pairing(event)) }
    func onAccountEvent(event: AccountEvent) { forward(.account(event)) }
    func onError(error: FsyncError) { forward(.error(error)) }

    func onSessionChanged(tokens: SessionTokens) {
        guard gate.isOpen else { return }
        do { try secrets.saveSession(tokens) } catch {
            Self.log.error("could not store refreshed session: \(String(describing: error), privacy: .private)")
        }
    }
}
