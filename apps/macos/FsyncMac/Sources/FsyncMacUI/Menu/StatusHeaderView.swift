import FsyncMacCore
import SwiftUI

/// Connection headline with a status dot, and the per-state counts below it.
struct StatusHeaderView: View {
    let summary: StatusSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Circle().fill(summary.indicatorColor).frame(width: 8, height: 8).accessibilityHidden(true)
                Text(summary.headline).font(.headline)
            }
            Text(summary.text).font(.caption).foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
