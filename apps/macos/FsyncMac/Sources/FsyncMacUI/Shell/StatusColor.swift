import FsyncMacCore
import SwiftUI

extension StatusSummary {
    /// Green when connected, orange when only seen online, grey otherwise. Shared by the toolbar pill and the popover.
    var indicatorColor: Color {
        if lan + relay > 0 { return .green }
        return present > 0 ? .orange : .secondary
    }
}
