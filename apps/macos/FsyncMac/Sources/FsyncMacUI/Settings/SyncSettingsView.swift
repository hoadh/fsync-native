import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// Clipboard mode and what macOS allows. Needs a signed-in Mac (the settings live in the engine).
struct SyncSettingsView: View {
    @ObservedObject var settings: ClipSettingsModel
    @State private var access = PasteboardAccess.current()

    var body: some View {
        Form {
            Section {
                Picker("Clipboard", selection: Binding(get: { settings.settings.clipMode }, set: { mode in
                    Task { await settings.update { $0.clipMode = mode } }
                })) {
                    Text("Share automatically").tag(ClipMode.auto)
                    Text("Only when I send").tag(ClipMode.manual)
                }
                .pickerStyle(.radioGroup)
                if let text = access.explanation, settings.settings.clipMode == .auto {
                    Text(text).font(.caption).foregroundStyle(.orange)
                }
            } footer: {
                Text(settings.settings.clipMode == .auto
                    ? "What you copy goes to your other devices, and what they copy lands on your clipboard."
                    : "Send with \(HotkeyManager.sendClipboardLabel) or the menu. Clips from other devices wait under \"Paste from\".")
            }
            Section {
                Toggle("Receive files from my devices without asking", isOn: Binding(
                    get: { settings.settings.autoAcceptFiles },
                    set: { value in Task { await settings.update { $0.autoAcceptFiles = value } } }))
            } header: {
                Text("Files")
            } footer: {
                Text("Files go to your Downloads folder. Otherwise each transfer waits for you under Transfers.")
            }
            if let error = settings.errorMessage {
                Section { Text(error).font(.caption).foregroundStyle(.red) }
            }
        }
        .formStyle(.grouped)
        .onAppear { access = PasteboardAccess.current() }
    }
}
