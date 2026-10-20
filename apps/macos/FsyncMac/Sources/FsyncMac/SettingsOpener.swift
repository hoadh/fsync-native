import AppKit
import FsyncMacUI
import SwiftUI

/// Content of the `settings-opener` scene. It never shows anything and never takes focus.
struct SettingsOpenerView: View {
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        Color.clear
            .frame(width: 1, height: 1)
            .background(WindowIdentifier(WindowIDs.settingsOpener, configure: OpenerWindowStyle.hide))
            .onReceive(SettingsRequest.publisher) { _ in showIfPending() }
            // A request made before this window existed was missed by `onReceive`.
            .onAppear { showIfPending() }
    }

    private func showIfPending() {
        if SettingsRequest.takePending() { show() }
    }

    private func show() {
        NSApp.activate()
        openSettings()
        front(attemptsLeft: 5)
    }

    /// The window is created during `openSettings()`, possibly a turn later: retry a few turns.
    private func front(attemptsLeft: Int) {
        DispatchQueue.main.async {
            if let settings = NSApp.windows.first(where: Self.isSettings) {
                settings.makeKeyAndOrderFront(nil)
                NSApp.activate()
            } else if attemptsLeft > 0 {
                front(attemptsLeft: attemptsLeft - 1)
            }
        }
    }

    private static func isSettings(_ window: NSWindow) -> Bool {
        guard let id = window.identifier, !WindowIDs.isSettingsOpener(window) else { return false }
        return id.rawValue.localizedCaseInsensitiveContains("settings")
    }
}
