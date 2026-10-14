import FsyncMacCore
import SwiftUI

/// Two steps before `resetAccountTrust()` runs. Cancel is the default button on both.
public struct RecoverySheet: View {
    @ObservedObject var model: AccountRecoveryModel

    public init(model: AccountRecoveryModel) {
        self.model = model
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            switch model.step {
            case .idle, .explain:
                Text("Lost your other devices?").font(.title3)
                Text("If you can't pair with another device, this Mac can become the only trusted device on your account. Every other device you own will have to confirm the change or pair again before it syncs with you.")
                buttons(primary: "Continue…") { model.proceed() }
            case .finalConfirm:
                Text("Reset trust for your account?").font(.title3)
                Text("Your other devices will stop syncing until you confirm the reset on them or pair them again. This can't be undone, and it can be done only a few times a day.")
                if let error = model.errorMessage { Text(error).foregroundStyle(.red).font(.callout) }
                buttons(primary: "Reset and trust only this Mac") { Task { await model.reset() } }
            }
        }
        .padding(24)
        .frame(width: 420)
    }

    private func buttons(primary: String, action: @escaping () -> Void) -> some View {
        HStack {
            Spacer()
            if model.isBusy { ProgressView().controlSize(.small) }
            Button(primary, role: .destructive, action: action).disabled(model.isBusy)
            // The default (Return) button is Cancel, never the destructive one.
            Button("Cancel") { model.cancel() }
                .keyboardShortcut(.defaultAction)
                .disabled(model.isBusy)
        }
    }
}
