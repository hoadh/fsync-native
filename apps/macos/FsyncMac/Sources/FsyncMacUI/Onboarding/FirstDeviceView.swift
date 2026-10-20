import SwiftUI

public struct FirstDeviceView: View {
    private let onDone: () -> Void

    public init(onDone: @escaping () -> Void) {
        self.onDone = onDone
    }

    public var body: some View {
        VStack(spacing: 14) {
            Image(systemName: "checkmark.circle.fill").font(.system(size: 40)).foregroundStyle(.green).accessibilityHidden(true)
            Text("This Mac is your first device").font(.title2)
            Text("Add others from their app with a pairing code.")
                .foregroundStyle(.secondary).multilineTextAlignment(.center)
            Button("Done", action: onDone).keyboardShortcut(.defaultAction)
        }
        .padding(30)
    }
}
