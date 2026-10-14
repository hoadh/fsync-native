import FsyncCoreKit
import Combine
import Foundation

// FsyncCoreKit exports its own `Published` struct, so the property wrapper is qualified below.

/// How the user's last answer ended.
public enum TrustResetOutcome: Equatable, Sendable {
    /// No answer given yet.
    case unanswered
    /// The new device list is now trusted (`Ready`, or `Created`).
    case trusted
    /// Trusted, but this Mac is not in the new list: it must be paired again.
    case removedNeedsPairing
    /// The reset was declined; the current devices stay trusted.
    case declined
}

/// State and actions behind the trust-reset prompt (ADR 012).
///
/// Engine calls block, so `confirm(resetId:)` and `decline(resetId:)` run them off the main actor. The app's
/// `EngineDelegate` adapter is called on a Rust thread and must hop to the main actor before
/// calling `handle(_:)`.
@MainActor
public final class TrustResetPromptModel: ObservableObject {
    /// Resets waiting for an answer; the prompt shows the first one. Tests may set it to
    /// simulate a stale list.
    @Combine.Published public internal(set) var pending: [TrustResetInfo] = []
    /// True while a confirm or decline call is running; further taps are ignored.
    @Combine.Published public private(set) var isBusy = false
    @Combine.Published public private(set) var outcome: TrustResetOutcome = .unanswered
    /// Short, user-readable text for the last failure.
    @Combine.Published public private(set) var errorMessage: String?
    /// True when `errorMessage` is informational (the request changed), not a failure.
    @Combine.Published public private(set) var errorIsNotice = false

    private let engine: FsyncEngineProtocol

    public init(engine: FsyncEngineProtocol) {
        self.engine = engine
    }

    /// Re-reads the pending resets from the engine.
    public func refresh() {
        apply(engine.pendingTrustResets())
    }

    /// Stores a new pending list. When the shown reset changes, the old message no longer
    /// applies; if one reset was replaced by another, the user is told to review it again.
    func apply(_ list: [TrustResetInfo]) {
        let oldId = pending.first?.resetId
        pending = list
        let newId = list.first?.resetId
        guard newId != oldId else { return }
        clearMessage()
        if oldId != nil, newId != nil { show(TrustResetMapping.listChanged) }
    }

    /// Refreshes on `.trustReset`; every other event is ignored. Call on the main actor.
    public func handle(_ event: AccountEvent) {
        if case .trustReset = event { refresh() }
    }

    /// Trusts exactly the reset with `resetId`, the one the user saw and confirmed. The id is
    /// never replaced by the current one: if it is no longer pending the engine says `NotFound`.
    public func confirm(resetId: String) {
        answer(resetId) { engine, id in
            TrustResetMapping.outcome(for: try engine.confirmTrustReset(resetId: id))
        }
    }

    /// Keeps this Mac's current devices; `resetId` is the reset the user saw.
    public func decline(resetId: String) {
        answer(resetId) { engine, id in
            try engine.declineTrustReset(resetId: id)
            return .declined
        }
    }

    /// Hides the outcome so the next pending reset (if any) shows.
    public func dismissOutcome() {
        outcome = .unanswered
        clearMessage()
    }

    private func answer(_ id: String, _ call: @escaping @Sendable (FsyncEngineProtocol, String) throws -> TrustResetOutcome) {
        guard !isBusy else { return }
        isBusy = true
        clearMessage()
        let engine = self.engine
        Task { [weak self] in
            let result = await Task.detached { Result { try call(engine, id) } }.value
            self?.finish(id, result)
        }
    }

    private func finish(_ id: String, _ result: Result<TrustResetOutcome, Error>) {
        switch result {
        case .success(let value):
            refresh()
            outcome = value
        case .failure(let error):
            if case FsyncError.NotFound = error {
                // Refresh first: it may clear a message when the shown reset changes.
                refresh()
                show(TrustResetMapping.message(for: error))
            } else if pending.first?.resetId == id {
                show(TrustResetMapping.message(for: error))
            }
            // Otherwise the shown reset changed during the call; its "list changed" notice stays.
        }
        isBusy = false
    }

    private func show(_ message: TrustResetMapping.Message) {
        errorMessage = message.text
        errorIsNotice = message.isNotice
    }

    private func clearMessage() {
        errorMessage = nil
        errorIsNotice = false
    }
}
