import Combine
import FsyncCoreKit
import Foundation

/// Email, then the 6-digit code. The model knows only the engine, the Keychain and a callback,
/// so a real engine is enough to test it.
@MainActor
public final class LoginModel: ObservableObject {
    public enum Step: Equatable, Sendable {
        case email
        case code(email: String)
    }

    public static let codeLength = 6
    public static let resendCooldown = 30

    @Combine.Published public internal(set) var step: Step = .email
    @Combine.Published public var email = ""
    @Combine.Published public var code = "" {
        didSet {
            let clean = Self.sanitizeCode(code)
            if clean != code { code = clean }
        }
    }
    @Combine.Published public private(set) var isBusy = false
    @Combine.Published public private(set) var errorMessage: String?
    @Combine.Published public private(set) var resendSecondsLeft = 0

    private let engine: FsyncEngineProtocol
    private let secrets: AppSecrets
    private let onSignedIn: @MainActor () -> Void
    private var cooldownTask: Task<Void, Never>?

    /// `onSignedIn` runs after the tokens are stored.
    public init(engine: FsyncEngineProtocol, secrets: AppSecrets, onSignedIn: @escaping @MainActor () -> Void) {
        self.engine = engine
        self.secrets = secrets
        self.onSignedIn = onSignedIn
    }

    deinit { cooldownTask?.cancel() }

    // MARK: Pure helpers

    /// Trimmed, contains "@", at most 254 characters.
    public static func isPlausibleEmail(_ raw: String) -> Bool {
        let e = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard e.count <= 254, let at = e.firstIndex(of: "@") else { return false }
        return at != e.startIndex && e.index(after: at) != e.endIndex
    }

    /// Digits only, at most `codeLength`.
    public static func sanitizeCode(_ raw: String) -> String {
        String(raw.filter { $0.isASCII && $0.isNumber }.prefix(codeLength))
    }

    public var canSendCode: Bool { !isBusy && Self.isPlausibleEmail(email) }
    public var canVerify: Bool { !isBusy && code.count == Self.codeLength }

    // MARK: Actions

    /// Sends the code (or sends it again) and moves to the code step.
    public func sendCode() async {
        guard canSendCode else { return }
        let address = email.trimmingCharacters(in: .whitespacesAndNewlines)
        isBusy = true
        errorMessage = nil
        let engine = self.engine
        let result = await EngineCall.run(engine) { try $0.loginStart(email: address) }
        isBusy = false
        switch result {
        case .success:
            step = .code(email: address)
            code = ""
            startCooldown()
        case .failure(let error):
            errorMessage = ErrorText.message(for: error, context: .login)
        }
    }

    public func resend() async {
        guard case .code(let address) = step, resendSecondsLeft == 0 else { return }
        email = address
        await sendCode()
    }

    /// Checks the code, stores the tokens, then hands over to the bridge.
    public func verify() async {
        guard case .code(let address) = step, canVerify else { return }
        let entered = code
        isBusy = true
        errorMessage = nil
        let engine = self.engine
        let result = await EngineCall.run(engine) { try $0.loginVerify(email: address, code: entered) }
        isBusy = false
        code = ""
        switch result {
        case .failure(let error):
            errorMessage = ErrorText.message(for: error, context: .login)
        case .success(let tokens):
            // Off the main actor; the session is stored before the phase can change.
            let secrets = secrets
            let saved = await Task.detached { Result { try secrets.saveSession(tokens) } }.value
            if case .failure = saved {
                // Best effort: do not leave a live server session that this Mac cannot keep.
                _ = await EngineCall.run(engine) { try $0.logout() }
                errorMessage = "Couldn't store your sign-in securely. Request a new code."
                return
            }
            reset()
            onSignedIn()
        }
    }

    public func useAnotherEmail() {
        step = .email
        code = ""
        errorMessage = nil
        stopCooldown()
    }

    /// Back to a blank email step (sign-out, session loss).
    public func reset() {
        useAnotherEmail()
        email = ""
    }

    // MARK: Cooldown (UX only; rate limits are server-side)

    private func startCooldown() {
        stopCooldown()
        resendSecondsLeft = Self.resendCooldown
        cooldownTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled, self.resendSecondsLeft > 0 else { return }
                self.resendSecondsLeft -= 1
            }
        }
    }

    private func stopCooldown() {
        cooldownTask?.cancel()
        cooldownTask = nil
        resendSecondsLeft = 0
    }
}
