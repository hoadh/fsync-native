import SwiftUI

/// Inline message at the top of a page (errors and offline notes). Click the dismiss button to clear it.
struct PageBanner: View {
    let text: String
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange).accessibilityHidden(true)
            Text(verbatim: text).font(.callout).frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onDismiss) { Image(systemName: "xmark") }
                .buttonStyle(.borderless).accessibilityLabel("Dismiss").help("Dismiss")
        }
        .padding(10)
        .background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .contain)
    }
}
