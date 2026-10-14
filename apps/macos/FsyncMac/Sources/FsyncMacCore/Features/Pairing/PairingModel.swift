import Combine
import FsyncCoreKit
import Foundation

public enum PairingRole: Equatable, Sendable {
    /// This Mac is new and shows a code or QR.
    case offer
    /// This Mac is trusted and enters the code shown on the new device.
    case join
}

public enum PairingState: Equatable, Sendable {
    case idle
    case offering(PairingOffer, PairingMode)
    case joining
    case sas(PairingPeer, PairingRole)
    /// The local answer is sent; the other side has not finished.
    case waitingForPeer
    case done(String)
    case failed(String)
}

/// Pairing with the SAS check, for both roles. Engine calls block, so they run off the main actor.
/// Every action bumps `generation`; a result from an older action is dropped.
@MainActor
public final class PairingModel: ObservableObject {
    @Combine.Published public internal(set) var state: PairingState = .idle
    /// True while an offer or join call is running.
    @Combine.Published public private(set) var isWorking = false
    /// Set when the join entry screen is open (before any call).
    @Combine.Published public private(set) var isJoinOpen = false

    private let engine: FsyncEngineProtocol
    private let onPaired: @MainActor (String) -> Void
    private let onSignInRequired: @MainActor () -> Void
    private var generation = 0

    public init(
        engine: FsyncEngineProtocol, onPaired: @escaping @MainActor (String) -> Void,
        onSignInRequired: @escaping @MainActor () -> Void
    ) {
        self.engine = engine
        self.onPaired = onPaired
        self.onSignInRequired = onSignInRequired
    }

    /// Anything on screen that needs the pairing flow instead of the normal home.
    public var isActive: Bool { isJoinOpen || state != .idle }

    public func openJoin() {
        guard state == .idle else { return }
        isJoinOpen = true
    }

    /// Opens an offer window. Any earlier offer is cancelled first, because the core allows one secret at a time.
    public func offer(_ mode: PairingMode) async {
        // One offer at a time: rapid taps would run two `pairingOffer` calls in either order.
        guard !isWorking else { return }
        // Switching Code/QR keeps the old offer on screen until the new one lands, so a SAS
        // request that arrives meanwhile still finds an `.offering` state to move on from.
        var current = PairingState.idle
        if case .offering = state { current = state }
        let gen = begin(current, working: true)
        let result = await EngineCall.run(engine) { engine in
            try? engine.pairingCancel()
            return try engine.pairingOffer(mode: mode)
        }
        guard gen == generation else {
            // Closed only if the user cancelled meanwhile; a SAS or result must keep its window.
            if case .success = result, state == .idle { await closeWindow() }
            return
        }
        isWorking = false
        switch result {
        case .success(let offer): state = .offering(offer, mode)
        case .failure(let error): fail(error, context: .general)
        }
    }

    /// Joins with a code or `fsync://pair` link. Returns when the SAS is ready.
    public func join(_ input: String) async {
        let code = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, state == .idle, !isWorking else { return }
        let gen = begin(.joining, working: true)
        let result = await EngineCall.run(engine) { try $0.pairingJoin(input: code) }
        guard gen == generation else { return }
        isWorking = false
        switch result {
        case .success(let peer): state = .sas(peer, .join)
        case .failure(let error): fail(error)
        }
    }

    /// Answers the SAS check for `requestId`, the request that was on screen. Anything else is ignored.
    public func confirm(requestId: UInt64, accept: Bool) async {
        guard case .sas(let peer, _) = state, peer.requestId == requestId else { return }
        let gen = begin(accept ? .waitingForPeer : .failed("Pairing cancelled. Nothing was paired."), working: false)
        let result = await EngineCall.run(engine) { try $0.pairingConfirm(requestId: requestId, accept: accept) }
        guard gen == generation, case .failure(let error) = result else { return }
        fail(error)
    }

    /// Ends whatever is open. Errors are ignored: the user is leaving.
    public func cancel() async {
        if case .done = state { return }  // keep the success screen until the user dismisses it
        let hadWindow: Bool
        switch state {
        case .offering, .joining, .sas, .waitingForPeer: hadWindow = true
        default: hadWindow = isWorking
        }
        _ = begin(.idle, working: false)
        isJoinOpen = false
        if hadWindow { await closeWindow() }
    }

    /// Leaves a finished or failed screen.
    public func dismiss() {
        guard state == .idle || state.isFinished else { return }
        _ = begin(.idle, working: false)
        isJoinOpen = false
    }

    /// Engine events. Pure state transitions plus the paired callback.
    public func handle(_ event: PairingEvent) {
        switch event {
        case .confirmRequest(let peer):
            // A newer request replaces the one on screen; an answer in flight for the old id is dropped.
            switch state {
            case .offering, .waitingForPeer:
                generation += 1
                isWorking = false
                state = .sas(peer, .offer)
            case .sas(_, let role):
                generation += 1
                state = .sas(peer, role)
            default: break
            }
        case .completed(let peer, _):
            generation += 1
            isWorking = false
            state = .done(PeerText.sanitize(peer))
            onPaired(PeerText.sanitize(peer))
        case .failed(let reason):
            // A late failure must not replace the success or failure already on screen.
            guard state != .idle, !state.isFinished else { return }
            generation += 1
            isWorking = false
            state = .failed(PeerText.sanitize(reason, limit: 120))
        }
    }

    private func begin(_ next: PairingState, working: Bool) -> Int {
        generation += 1
        isWorking = working
        state = next
        return generation
    }

    private func fail(_ error: Error, context: ErrorContext = .pairing) {
        if ErrorText.requiresSignIn(error) {
            state = .idle
            isJoinOpen = false
            return onSignInRequired()
        }
        // Cancelled is silent: the user ended it.
        state = ErrorText.message(for: error, context: context).map { .failed($0) } ?? .idle
    }

    private func closeWindow() async {
        _ = await EngineCall.run(engine) { try $0.pairingCancel() }
    }
}

extension PairingState {
    var isFinished: Bool {
        switch self {
        case .done, .failed: return true
        default: return false
        }
    }
}
