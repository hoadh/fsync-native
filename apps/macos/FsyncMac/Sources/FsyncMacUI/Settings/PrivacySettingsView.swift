import FsyncMacCore
import SwiftUI

/// Apps whose copies are never shared. The built-in password managers can't be removed.
struct PrivacySettingsView: View {
    @ObservedObject var settings: ClipSettingsModel
    @State private var newApp = ""

    var body: some View {
        Form {
            Section {
                ForEach(SensitiveFilter.builtInDenylist.sorted(), id: \.self) { id in
                    Text(verbatim: "\(id)  (built in)").foregroundStyle(.secondary)
                }
                ForEach(settings.settings.denylist, id: \.self) { id in
                    LabeledContent {
                        Button { Task { await settings.removeFromDenylist(id) } } label: { Image(systemName: "minus.circle") }
                            .buttonStyle(.borderless).accessibilityLabel("Remove \(id)")
                    } label: { Text(verbatim: id) }
                }
            } header: {
                Text("Never share copies from these apps")
            } footer: {
                Text("Items marked concealed or temporary by an app are never shared.")
            }
            Section {
                HStack {
                    TextField("Bundle id, e.g. com.example.vault", text: $newApp).textFieldStyle(.roundedBorder)
                    Button("Add") {
                        let id = newApp
                        newApp = ""
                        Task { await settings.addToDenylist(id) }
                    }
                    .disabled(newApp.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            if let error = settings.errorMessage {
                Section { Text(error).font(.caption).foregroundStyle(.red) }
            }
        }
        .formStyle(.grouped)
    }
}
