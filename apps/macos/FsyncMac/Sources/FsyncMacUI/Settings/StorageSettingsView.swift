import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// How long history is kept and how big a clip may be.
struct StorageSettingsView: View {
    @ObservedObject var settings: ClipSettingsModel
    @State private var strategy = FileClipStrategy.stored()

    var body: some View {
        Form {
            Stepper(value: days, in: 1...3650) { Text("Keep history for \(settings.settings.retentionDays) days") }
            Stepper(value: megabytes(\.retentionBytes), in: 16...102_400, step: 64) {
                Text("History size limit: \(Self.label(settings.settings.retentionBytes))")
            }
            Stepper(value: megabytes(\.maxClipBytes), in: 1...4096, step: 10) {
                Text("Largest clip: \(Self.label(settings.settings.maxClipBytes))")
            }
            Picker("Files you paste", selection: Binding(get: { strategy }, set: { strategy = $0; $0.store() })) {
                Text("Small files copied now, large ones when pasted").tag(FileClipStrategy.hybrid)
                Text("Always when pasted").tag(FileClipStrategy.lazy)
                Text("Always copied now").tag(FileClipStrategy.eager)
            }
            Text("Applies to the next file clip. Copied-now files are kept in a cache folder and removed with your history.")
                .font(.caption).foregroundStyle(.secondary)
            Text("Pinned items are kept until you delete them.").font(.caption).foregroundStyle(.secondary)
            if let error = settings.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .padding(20)
    }

    private var days: Binding<Int> {
        Binding(get: { Int(settings.settings.retentionDays) }, set: { value in
            Task { await settings.update { $0.retentionDays = UInt32(clamping: value) } }
        })
    }

    private func megabytes(_ key: WritableKeyPath<FsyncCoreKit.Settings, UInt64>) -> Binding<Int> {
        Binding(get: { Int(settings.settings[keyPath: key] >> 20) }, set: { value in
            Task { await settings.update { $0[keyPath: key] = UInt64(max(value, 1)) << 20 } }
        })
    }

    static func label(_ bytes: UInt64) -> String { ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .binary) }
}
