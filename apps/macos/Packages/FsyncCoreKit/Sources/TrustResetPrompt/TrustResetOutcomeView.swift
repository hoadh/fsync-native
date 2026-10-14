import SwiftUI

/// Result screen shown after the user answered a trust reset.
struct TrustResetOutcomeView: View {
    let outcome: TrustResetOutcome
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: symbol).font(.largeTitle).foregroundColor(color)
                .accessibilityHidden(true)
            Text(title).font(.title3.bold()).accessibilityAddTraits(.isHeader)
            Text(detail).multilineTextAlignment(.center).foregroundColor(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Button("Done", action: onDone).keyboardShortcut(.defaultAction)
        }
        .frame(maxWidth: .infinity)
    }

    private var symbol: String {
        switch outcome {
        case .trusted: return "checkmark.shield.fill"
        case .removedNeedsPairing: return "exclamationmark.triangle.fill"
        case .declined, .unanswered: return "lock.shield.fill"
        }
    }

    private var color: Color {
        outcome == .removedNeedsPairing ? .orange : .green
    }

    private var title: String {
        switch outcome {
        case .trusted: return "New devices trusted"
        case .removedNeedsPairing: return "Pair this Mac again"
        case .declined, .unanswered: return "Current devices kept"
        }
    }

    private var detail: String {
        switch outcome {
        case .trusted: return "This Mac now trusts the new device list."
        case .removedNeedsPairing: return "This Mac is not in the new list; pair it again."
        case .declined, .unanswered: return "This Mac keeps trusting its current devices."
        }
    }
}
