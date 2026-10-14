import FsyncCoreKit
import SwiftUI

/// The trust-reset prompt backed by a `TrustResetPromptModel`.
///
/// Shows the first pending reset, or the outcome of the last answer, or an empty state.
public struct TrustResetPromptView: View {
    @ObservedObject private var model: TrustResetPromptModel
    private let onDone: () -> Void

    /// `onDone` is called when the user dismisses the outcome or the empty state.
    public init(model: TrustResetPromptModel, onDone: @escaping () -> Void) {
        self.model = model
        self.onDone = onDone
    }

    public var body: some View {
        TrustResetPromptContent(
            info: model.pending.first,
            outcome: model.outcome,
            isBusy: model.isBusy,
            errorMessage: model.errorMessage,
            errorIsNotice: model.errorIsNotice,
            onKeep: { model.decline(resetId: $0) },
            onTrust: { model.confirm(resetId: $0) },
            onDone: {
                model.dismissOutcome()
                onDone()
            }
        )
        .onAppear { model.refresh() }
    }
}

/// Stateless prompt: a function of plain state plus callbacks, so every state can be rendered.
public struct TrustResetPromptContent: View {
    let info: TrustResetInfo?
    let outcome: TrustResetOutcome
    let isBusy: Bool
    let errorMessage: String?
    let errorIsNotice: Bool
    /// Called with the id of the reset that was on screen when the user answered.
    let onKeep: (String) -> Void
    let onTrust: (String) -> Void
    let onDone: () -> Void

    public init(
        info: TrustResetInfo?, outcome: TrustResetOutcome, isBusy: Bool, errorMessage: String?,
        errorIsNotice: Bool = false, onKeep: @escaping (String) -> Void, onTrust: @escaping (String) -> Void, onDone: @escaping () -> Void
    ) {
        self.info = info
        self.outcome = outcome
        self.isBusy = isBusy
        self.errorMessage = errorMessage
        self.errorIsNotice = errorIsNotice
        self.onKeep = onKeep
        self.onTrust = onTrust
        self.onDone = onDone
    }

    public var body: some View {
        Group {
            if outcome != .unanswered {
                TrustResetOutcomeView(outcome: outcome, onDone: onDone)
            } else if let info {
                // New identity per reset: any open confirmation dialog closes when the id changes.
                TrustResetPendingView(
                    info: info, isBusy: isBusy, errorMessage: errorMessage,
                    errorIsNotice: errorIsNotice, onKeep: onKeep, onTrust: onTrust
                ).id(info.resetId)
            } else {
                emptyView
            }
        }
        .padding(24)
        .frame(width: 440)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var emptyView: some View {
        VStack(spacing: 12) {
            Image(systemName: "checkmark.shield").font(.largeTitle).foregroundColor(.secondary)
                .accessibilityHidden(true)
            Text("No trust reset is waiting for you.").font(.headline)
            Button("Done", action: onDone).keyboardShortcut(.defaultAction)
        }
        .frame(maxWidth: .infinity)
    }
}

#if DEBUG
#Preview("Pending") {
    TrustResetPromptContent(
        info: TrustResetInfo(
            resetId: String(repeating: "ab12cd34", count: 8), devices: 2,
            deviceNames: ["Alex's MacBook Pro", "Alex's iPhone"]),
        outcome: .unanswered, isBusy: false, errorMessage: nil, onKeep: { _ in }, onTrust: { _ in }, onDone: {})
}
#endif
