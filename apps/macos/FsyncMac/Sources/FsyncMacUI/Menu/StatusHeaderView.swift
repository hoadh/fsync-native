import FsyncMacCore
import SwiftUI

struct StatusHeaderView: View {
    let summary: StatusSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(summary.headline).font(.headline)
            Text(summary.text).font(.caption).foregroundStyle(.secondary)
        }
    }
}
