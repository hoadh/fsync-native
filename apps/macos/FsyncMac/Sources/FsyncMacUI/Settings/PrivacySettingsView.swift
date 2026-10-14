import FsyncMacCore
import SwiftUI

/// Apps whose copies are never shared. The built-in password managers can't be removed.
struct PrivacySettingsView: View {
    @ObservedObject var settings: ClipSettingsModel
    @State private var newApp = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Never share copies from these apps").font(.headline)
            List {
                ForEach(SensitiveFilter.builtInDenylist.sorted(), id: \.self) { id in
                    Text(verbatim: "\(id)  (built in)").foregroundStyle(.secondary)
                }
                ForEach(settings.settings.denylist, id: \.self) { id in
                    HStack {
                        Text(verbatim: id)
                        Spacer()
                        Button { Task { await settings.removeFromDenylist(id) } } label: { Image(systemName: "minus.circle") }
                            .buttonStyle(.borderless)
                    }
                }
            }
            HStack {
                TextField("Bundle id, e.g. com.example.vault", text: $newApp).textFieldStyle(.roundedBorder)
                Button("Add") {
                    let id = newApp
                    newApp = ""
                    Task { await settings.addToDenylist(id) }
                }
                .disabled(newApp.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            Text("Items marked concealed or temporary by an app are never shared.").font(.caption).foregroundStyle(.secondary)
            if let error = settings.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .padding(20)
    }
}
