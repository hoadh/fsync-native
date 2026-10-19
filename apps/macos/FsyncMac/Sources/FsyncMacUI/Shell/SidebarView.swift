import FsyncMacCore
import SwiftUI

/// Pages of the main window. The selection is optional because `List(selection:)` needs `MainSection?`.
struct SidebarView: View {
    @ObservedObject var navigation: MainNavigation
    @ObservedObject var files: FilesSync
    let onPair: () -> Void
    let signOut: () -> Void

    var body: some View {
        List(selection: Binding(get: { navigation.section ?? .devices }, set: { if let chosen = $0 { navigation.section = chosen } })) {
            ForEach(Self.sections, id: \.self) { section in
                row(section)
            }
        }
        .navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 260)
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                Button(action: onPair) { Label("Pair device…", systemImage: "plus.circle") }
                    .buttonStyle(.borderless)
                SignOutButton(action: signOut).buttonStyle(.borderless).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
        }
    }

    @ViewBuilder private func row(_ section: MainSection) -> some View {
        let label = Label(Self.title(section), systemImage: Self.symbol(section))
        if section == .transfers, !files.offers.isEmpty {
            label.tag(section).badge(files.offers.count)
        } else {
            label.tag(section)
        }
    }

    /// The rows, in order.
    static let sections = MainSection.allCases

    static func title(_ section: MainSection) -> String {
        switch section {
        case .devices: "Devices"
        case .history: "History"
        case .transfers: "Transfers"
        }
    }

    static func symbol(_ section: MainSection) -> String {
        switch section {
        case .devices: "laptopcomputer.and.iphone"
        case .history: "clock.arrow.circlepath"
        case .transfers: "arrow.up.arrow.down.circle"
        }
    }
}
