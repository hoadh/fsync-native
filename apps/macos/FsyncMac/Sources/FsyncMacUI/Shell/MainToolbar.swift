import FsyncMacCore
import SwiftUI

/// Toolbar of the main window: a static connection status and "Pair device…".
struct MainToolbar: ToolbarContent {
    let summary: StatusSummary
    let onPair: () -> Void

    var body: some ToolbarContent {
        ToolbarItem(placement: .navigation) { StatusPill(summary: summary) }
        ToolbarItem(placement: .primaryAction) {
            Button(action: onPair) { Label("Pair device…", systemImage: "plus") }
                .help("Pair a new device")
        }
    }
}

/// "Connected" / "Online" / "No devices online" as a capsule; the detail text is the tooltip.
struct StatusPill: View {
    let summary: StatusSummary

    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(summary.indicatorColor).frame(width: 8, height: 8).accessibilityHidden(true)
            Text(summary.headline).font(.callout)
        }
        .padding(.horizontal, 10).padding(.vertical, 3)
        .background(.quaternary, in: Capsule())
        .help(summary.text)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "\(summary.headline). \(summary.text)"))
    }
}
