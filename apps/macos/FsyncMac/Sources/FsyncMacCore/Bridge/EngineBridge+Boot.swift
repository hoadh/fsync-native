import FsyncCoreKit
import Foundation

extension EngineBridge {
    /// Starts booting in the background; observe `phase`. Calling it again restarts the account check.
    public func boot() {
        bootTask?.cancel()
        phase = .booting
        banner = startupNotice
        bootTask = Task { [weak self] in await self?.runBoot() }
    }

    private func runBoot() async {
        if !pendingRevokeTried {
            pendingRevokeTried = true
            resumePendingRevoke()
        }
        if engine == nil, !(await makeEngine()) { return }
        guard let engine else { return }
        // Keychain reads can block (even on a system prompt), so never on the main actor.
        let secrets = secrets
        let read = await Task.detached { Result { try secrets.session() } }.value
        guard !Task.isCancelled else { return }
        let stored: SessionTokens?
        switch read {
        case .success(let tokens): stored = tokens
        case .failure:
            phase = .bootFailed("Can't read your saved sign-in from the Keychain.", canReset: false)
            return
        }
        // Open before `start()`: it can begin authenticated work, and a refresh it triggers rotates
        // the refresh token, so the new pair must reach the Keychain or the next launch signs out.
        sessionGate.set(open: stored != nil)
        let wasStarted = started
        let hook = beforeEngineStart
        let result = await EngineCall.run(engine) { engine in
            hook?()
            if let stored { try engine.setSession(tokens: stored) }
            if !wasStarted { try engine.start() }
        }
        if case .success = result { started = true }
        // A cancelled boot is stale (sign-out, new sign-in): it must not write phase or banner.
        guard !Task.isCancelled else { return }
        if case .failure(let error) = result {
            sessionGate.set(open: false)
            return fail(error)
        }
        if stored == nil { phase = .signedOut } else { await resolveAccount() }
    }

    /// Builds the engine off the main actor. False when boot failed (phase already set).
    private func makeEngine() async -> Bool {
        let (config, secrets, dataDir, gate) = (config, secrets, dataDir, sessionGate)
        let forward: @Sendable (EngineEvent) -> Void = { [weak self] event in
            // FIFO main queue keeps callback order; a Task per event would not.
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.receive(event) } }
        }
        let result = await Task.detached { () -> Result<FsyncEngine, Error> in
            Result {
                let cfg = EngineConfig(
                    dataDir: dataDir.path, apiBase: config.apiBase, relayUrl: config.relayUrl,
                    deviceName: config.deviceName, platform: .mac,
                    deviceSecret: try secrets.loadOrCreateKey(AppSecrets.deviceSecretAccount),
                    historyKey: try secrets.loadOrCreateKey(AppSecrets.historyKeyAccount),
                    forceRelay: false, allowInsecureHttp: config.allowInsecureHttp)
                return try FsyncEngine(cfg: cfg, delegate: EngineDelegateAdapter(secrets: secrets, gate: gate, forward: forward))
            }
        }.value
        switch result {
        case .success(let engine):
            self.engine = engine
            loginModel = LoginModel(engine: engine, secrets: secrets) { [weak self] in self?.didSignIn() }
            recoveryModel = AccountRecoveryModel(
                engine: engine, onResolved: { [weak self] in self?.applyAccountState($0) },
                onSignInRequired: { [weak self] in self?.requireSignIn(banner: "Sign in again to continue.") })
            installAccountModels(engine: engine)
            return true
        case .failure(let error):
            if !Task.isCancelled { fail(error) }
            return false
        }
    }

    /// Asks the server for the account state; while offline keeps retrying with backoff.
    func resolveAccount() async {
        var attempt = 0
        while !Task.isCancelled, let engine {
            let result = await EngineCall.run(engine, { try $0.ensureAccount() })
            guard !Task.isCancelled else { return }
            switch result {
            case .success(let state):
                applyAccountState(state)
                trustModel?.refresh()
                reloadDevices()
                return
            case .failure(let error):
                guard case FsyncError.Network = error else { return fail(error) }
                banner = ErrorText.message(for: error, context: .general)
                let delay = retryDelays[min(attempt, retryDelays.count - 1)]
                attempt += 1
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    private func fail(_ error: Error) {
        if ErrorText.requiresSignIn(error) { return requireSignIn() }
        let canReset: Bool
        if case FsyncError.HistoryKeyMismatch = error { canReset = true } else { canReset = false }
        let text = ErrorText.message(for: error, context: .general) ?? "Something went wrong."
        phase = .bootFailed(text, canReset: canReset)
    }
}
