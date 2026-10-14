import FsyncMacCore
import SwiftUI

public struct OtpView: View {
    @ObservedObject var model: LoginModel
    let email: String
    @FocusState private var focused: Bool

    public init(model: LoginModel, email: String) {
        self.model = model
        self.email = email
    }

    public var body: some View {
        VStack(spacing: 14) {
            Text("Enter your code").font(.title2)
            Text("Sent to \(email)").foregroundStyle(.secondary)
            TextField("123456", text: $model.code)
                .textContentType(.oneTimeCode)
                .textFieldStyle(.roundedBorder)
                .multilineTextAlignment(.center)
                .font(.system(.title3, design: .monospaced))
                .frame(maxWidth: 160)
                .focused($focused)
                .disabled(model.isBusy)
                .onSubmit { Task { await model.verify() } }
                .onChange(of: model.code) { value in
                    if value.count == LoginModel.codeLength { Task { await model.verify() } }
                }
            if model.isBusy { ProgressView().controlSize(.small) }
            if let error = model.errorMessage { Text(error).foregroundStyle(.red).font(.callout) }
            HStack {
                Button(model.resendSecondsLeft > 0 ? "Resend code (\(model.resendSecondsLeft)s)" : "Resend code") {
                    Task { await model.resend() }
                }
                .disabled(model.resendSecondsLeft > 0 || model.isBusy)
                Button("Use another email") { model.useAnotherEmail() }
            }
            .buttonStyle(.link)
        }
        .onAppear { focused = true }
    }
}
