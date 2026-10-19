import FsyncMacCore
import SwiftUI

/// Email, then the code. Which one shows follows `model.step`.
struct LoginFlowView: View {
    @ObservedObject var model: LoginModel
    let banner: String?

    var body: some View {
        VStack(spacing: 14) {
            if let banner { Text(banner).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.center) }
            switch model.step {
            case .email: LoginView(model: model)
            case .code(let email): OtpView(model: model, email: email)
            }
        }
        .padding(30)
    }
}

public struct LoginView: View {
    @ObservedObject var model: LoginModel
    @FocusState private var focused: Bool

    public init(model: LoginModel) {
        self.model = model
    }

    public var body: some View {
        VStack(spacing: 14) {
            Image(systemName: "arrow.triangle.2.circlepath").font(.system(size: 32)).foregroundStyle(.tint).accessibilityHidden(true)
            Text("Sign in to fsync").font(.title2)
            Text("We'll email you a 6-digit code.").foregroundStyle(.secondary)
            TextField("Email", text: $model.email)
                .textFieldStyle(.roundedBorder)
                .focused($focused)
                .onSubmit { Task { await model.sendCode() } }
                .disabled(model.isBusy)
            if let error = model.errorMessage {
                Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red).font(.callout)
            }
            Button("Send code") { Task { await model.sendCode() } }
                .keyboardShortcut(.defaultAction)
                .disabled(!model.canSendCode)
        }
        .frame(maxWidth: 340)
        .onAppear { focused = true }
    }
}
