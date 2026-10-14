import SwiftUI
import TrustResetPrompt

/// Shows the trust-reset prompt instead of `content` while a reset waits or its outcome is on screen.
struct TrustGate<Content: View>: View {
    @ObservedObject var trust: TrustResetPromptModel
    @ViewBuilder let content: Content

    var body: some View {
        if !trust.pending.isEmpty || trust.outcome != .unanswered {
            TrustResetPromptView(model: trust, onDone: {})
        } else {
            content
        }
    }
}
