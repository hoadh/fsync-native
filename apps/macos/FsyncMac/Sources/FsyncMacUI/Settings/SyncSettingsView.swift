import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Clipboard mode and what macOS allows. Needs a signed-in Mac (the settings live in the engine).
struct SyncSettingsView: View {
    @ObservedObject var settings: ClipSettingsModel
    @State private var access = PasteboardAccess.current()

    var body: some View {
        Form {
            Picker("Clipboard", selection: Binding(get: { settings.settings.clipMode }, set: { mode in
                Task { await settings.update { $0.clipMode = mode } }
            })) {
                Text("Share automatically").tag(ClipMode.auto)
                Text("Only when I send").tag(ClipMode.manual)
            }
            .pickerStyle(.radioGroup)
            Text(settings.settings.clipMode == .auto
                ? "What you copy goes to your other devices, and what they copy lands on your clipboard."
                : "Send with \(HotkeyManager.sendClipboardLabel) or the menu. Clips from other devices wait under \"Paste from\".")
                .font(.caption).foregroundStyle(.secondary)
            if let text = access.explanation, settings.settings.clipMode == .auto {
                Text(text).font(.caption).foregroundStyle(.orange)
            }
            if let error = settings.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .padding(20)
        .onAppear { access = PasteboardAccess.current() }
    }
}
