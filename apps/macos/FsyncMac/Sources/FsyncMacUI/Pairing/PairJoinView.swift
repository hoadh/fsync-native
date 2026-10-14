import FsyncMacCore
import SwiftUI

/// This Mac is trusted: enter the code shown on the new device.
struct PairJoinView: View {
    @ObservedObject var model: PairingModel
    @State private var code = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 14) {
            Text("Add a device").font(.title2)
            Text("On the new device, choose “Pair this device” and enter the code it shows.")
                .foregroundStyle(.secondary).multilineTextAlignment(.center)
            TextField("Pairing code or link", text: $code)
                .textFieldStyle(.roundedBorder).frame(width: 260).focused($focused)
                .onSubmit { Task { await model.join(code) } }
            HStack {
                Button("Cancel") { Task { await model.cancel() } }
                Button("Connect") { Task { await model.join(code) } }
                    .keyboardShortcut(.defaultAction)
                    .disabled(code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .onAppear { focused = true }
    }
}

/// `pairing_join` blocks until the other device answers; the user can always cancel.
struct PairJoiningView: View {
    @ObservedObject var model: PairingModel
    @State private var since = Date()

    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
            TimelineView(.periodic(from: since, by: 1)) { context in
                Text("Connecting… \(Int(context.date.timeIntervalSince(since))) s").foregroundStyle(.secondary).monospacedDigit()
            }
            Button("Cancel") { Task { await model.cancel() } }
        }
    }
}
