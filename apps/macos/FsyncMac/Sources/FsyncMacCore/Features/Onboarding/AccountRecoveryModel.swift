import Combine
import FsyncCoreKit
import Foundation

/// "Lost your other devices?": for a Mac that needs pairing when the user has no device to pair with.
/// `resetAccountTrust()` makes this Mac the account's only trusted device, so every other device
/// must confirm the reset or pair again. It is destructive, so it takes two explicit steps.
@MainActor
public final class AccountRecoveryModel: ObservableObject {
    public enum Step: Equatable, Sendable {
        case idle
        /// Explains what will happen.
        case explain
        /// Last chance; only here does `reset()` call the engine.
        case finalConfirm
    }

    @Combine.Published public private(set) var step: Step = .idle
    @Combine.Published public private(set) var isBusy = false
    @Combine.Published public private(set) var errorMessage: String?

    private let engine: FsyncEngineProtocol
    private let onResolved: @MainActor (AccountState) -> Void
    private let onSignInRequired: @MainActor () -> Void

    /// `onResolved` gets the new account state (mapped to a phase by the bridge); `onSignInRequired`
    /// runs when the server wants a fresh login (`ReauthRequired`, `Auth`).
    public init(
        engine: FsyncEngineProtocol, onResolved: @escaping @MainActor (AccountState) -> Void,
        onSignInRequired: @escaping @MainActor () -> Void
    ) {
        self.engine = engine
        self.onResolved = onResolved
        self.onSignInRequired = onSignInRequired
    }

    public var isPresented: Bool { step != .idle }

    public func begin() {
        guard !isBusy else { return }
        errorMessage = nil
        step = .explain
    }

    /// Moves from the explanation to the final confirmation; ignored from any other step.
    public func proceed() {
        guard step == .explain, !isBusy else { return }
        step = .finalConfirm
    }

    public func cancel() {
        guard !isBusy else { return }
        step = .idle
        errorMessage = nil
    }

    /// Calls `resetAccountTrust()`, only from the final confirmation.
    public func reset() async {
        guard step == .finalConfirm, !isBusy else { return }
        isBusy = true
        errorMessage = nil
        let result = await EngineCall.run(engine) { try $0.resetAccountTrust() }
        isBusy = false
        switch result {
        case .success(let state):
            step = .idle
            onResolved(state)
        case .failure(let error):
            if ErrorText.requiresSignIn(error) {
                step = .idle
                onSignInRequired()
            } else {
                errorMessage = ErrorText.message(for: error, context: .general)
            }
        }
    }
}
