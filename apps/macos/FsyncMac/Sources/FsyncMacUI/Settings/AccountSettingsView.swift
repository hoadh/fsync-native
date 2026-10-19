import FsyncMacCore
import SwiftUI

/// Who is signed in, this Mac's node prefix, and the confirmed sign-out.
struct AccountSettingsView: View {
    @ObservedObject var bridge: EngineBridge
    @State private var nodePrefix: String?

    /// A boot failure or a pending boot is not "signed in": the session state is unknown.
    private var isSignedIn: Bool { bridge.phase == .ready || bridge.phase == .needsPairing }

    private var statusText: String {
        switch bridge.phase {
        case .ready, .needsPairing: "Signed in"
        case .signedOut: "Signed out"
        case .booting: "Starting…"
        case .bootFailed: "Unavailable"
        }
    }

    /// The address typed at sign-in; known only until the next launch (the session stores no email).
    private var email: String? {
        let value = bridge.loginModel?.email.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return isSignedIn && !value.isEmpty ? value : nil
    }

    var body: some View {
        Form {
            Section {
                LabeledContent("Status", value: statusText)
                if let email { LabeledContent("Email") { Text(verbatim: email).textSelection(.enabled) } }
                if let nodePrefix {
                    LabeledContent("This Mac") { Text(verbatim: "\(nodePrefix)…").monospaced().textSelection(.enabled) }
                }
            }
            Section {
                SignOutButton(isDisabled: !isSignedIn) { Task { await bridge.signOut() } }
            } footer: {
                Text("Your device keys and clipboard history stay on this Mac.")
            }
        }
        .formStyle(.grouped)
        .task { nodePrefix = await bridge.nodeIdPrefix() }
    }
}
