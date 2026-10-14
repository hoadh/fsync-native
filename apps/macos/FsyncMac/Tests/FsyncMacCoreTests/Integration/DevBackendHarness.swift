import Foundation
import XCTest

/// Environment exported by `apps/macos/scripts/run-fsync-mac-integration.sh`. Without `FSYNC_IT_API`
/// the integration tests skip, so a plain `swift test` stays hermetic.
struct DevBackend: Sendable {
    let api: String
    let relay: String
    let backendLog: URL
    let cli: URL
    let work: URL

    static func fromEnvironment() throws -> DevBackend {
        let env = ProcessInfo.processInfo.environment
        guard let api = env["FSYNC_IT_API"], !api.isEmpty else {
            throw XCTSkip("FSYNC_IT_API is not set; run apps/macos/scripts/run-fsync-mac-integration.sh")
        }
        func need(_ key: String) throws -> String {
            guard let v = env[key], !v.isEmpty else { throw XCTSkip("\(key) is not set") }
            return v
        }
        return DevBackend(
            api: api, relay: try need("FSYNC_IT_RELAY"), backendLog: URL(fileURLWithPath: try need("FSYNC_IT_BACKEND_LOG")),
            cli: URL(fileURLWithPath: try need("FSYNC_IT_CLI")), work: URL(fileURLWithPath: try need("FSYNC_IT_WORK")))
    }

    /// Newest dev OTP logged for `email` (ANSI stripped), waiting up to `timeout` for it to appear.
    func otp(for email: String, timeout: Double = 10) async throws -> String {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if let text = try? String(contentsOf: backendLog, encoding: .utf8),
                let code = Self.lastOtp(in: text, email: email)
            { return code }
            try await Task.sleep(nanoseconds: 100_000_000)
        } while Date() < deadline
        throw HarnessError.noOtp(email)
    }

    static func lastOtp(in log: String, email: String) -> String? {
        let ansi = try? NSRegularExpression(pattern: "\u{1B}\\[[0-9;]*m")
        let code = try? NSRegularExpression(pattern: "code=\"([0-9]+)\"")
        var found: String?
        for raw in log.split(separator: "\n") where raw.contains("dev otp") {
            let line = String(raw)
            let range = NSRange(line.startIndex..., in: line)
            let clean = ansi?.stringByReplacingMatches(in: line, range: range, withTemplate: "") ?? line
            guard clean.contains("to=\"\(email)\"") else { continue }
            let r = NSRange(clean.startIndex..., in: clean)
            if let m = code?.firstMatch(in: clean, range: r), let g = Range(m.range(at: 1), in: clean) {
                found = String(clean[g])
            }
        }
        return found
    }

    /// `fsync-cli` for device `name` (its own data dir), global flags first.
    func cli(_ name: String, _ args: [String]) throws -> CliProcess {
        try CliProcess(
            executable: cli,
            arguments: ["--data", work.appendingPathComponent("cli-\(name)").path, "--api", api, "--relay", relay,
                        "--insecure-http", "--name", "dev\(name)"] + args)
    }

    /// Sends a code, reads it from the backend log and verifies it. Returns the `logged_in` event.
    func cliLogin(_ name: String, email: String) async throws -> CliEvent {
        let send = try cli(name, ["login", "--email", email, "--send-only"])
        try await send.expectExit(0, timeout: 30)
        let code = try await otp(for: email)
        let login = try cli(name, ["login", "--email", email, "--code", code])
        try await login.expectExit(0, timeout: 60)
        return try XCTUnwrap(login.lastEvent("logged_in"), "no logged_in event\n\(login.diagnostics)")
    }
}

enum HarnessError: Error, CustomStringConvertible {
    case noOtp(String)
    case timeout(String)
    case exit(Int32?, String)

    var description: String {
        switch self {
        case .noOtp(let email): return "no dev OTP in the backend log for \(email)"
        case .timeout(let what): return "timed out: \(what)"
        case .exit(let code, let diag): return "exit \(code.map(String.init) ?? "none")\n\(diag)"
        }
    }
}

/// A running `fsync-cli`; stdout is parsed as JSON lines.
final class CliProcess: @unchecked Sendable {
    private let process = Process()
    private let lock = NSLock()
    private var lines: [CliEvent] = []
    private var raw: [String] = []
    private var errText = ""
    private var buffer = Data()
    private var exited: Int32?
    private var exitedAt: Date?
    private let readers: [FileHandle]   // stdout, stderr
    /// After exit, how long open pipes (e.g. held by a grandchild) may delay reporting it.
    private static let drainGrace: TimeInterval = 1
    private var eofStreams = 0   // stdout and stderr each reach EOF once

    init(executable: URL, arguments: [String]) throws {
        process.executableURL = executable
        process.arguments = arguments
        let out = Pipe(), err = Pipe()
        readers = [out.fileHandleForReading, err.fileHandleForReading]
        process.standardOutput = out
        process.standardError = err
        process.standardInput = FileHandle.nullDevice
        out.fileHandleForReading.readabilityHandler = { [weak self] h in
            let d = h.availableData
            if d.isEmpty { h.readabilityHandler = nil; self?.finishStdout(); return }
            self?.append(d)
        }
        err.fileHandleForReading.readabilityHandler = { [weak self] h in
            let d = h.availableData
            if d.isEmpty { h.readabilityHandler = nil; self?.sync { $0.eofStreams += 1 }; return }
            self?.sync { $0.errText += String(decoding: d, as: UTF8.self) }
        }
        process.terminationHandler = { [weak self] p in self?.sync { $0.exited = p.terminationStatus; $0.exitedAt = Date() } }
        try process.run()
    }

    private func sync(_ body: (CliProcess) -> Void) { lock.lock(); body(self); lock.unlock() }

    private func record(line: Data) {   // call with the lock held
        raw.append(String(decoding: line, as: UTF8.self))
        if let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any] { lines.append(CliEvent(obj)) }
    }

    private func append(_ data: Data) {
        sync { s in
            s.buffer.append(data)
            while let nl = s.buffer.firstIndex(of: 0x0A) {
                let line = s.buffer.subdata(in: s.buffer.startIndex..<nl)
                s.buffer.removeSubrange(s.buffer.startIndex...nl)
                s.record(line: line)
            }
        }
    }

    /// stdout reached EOF: parse a final line that had no trailing newline, then mark the stream drained.
    private func finishStdout() {
        sync { s in
            if !s.buffer.isEmpty { s.record(line: s.buffer); s.buffer = Data() }
            s.eofStreams += 1
        }
    }

    func lastEvent(_ name: String) -> CliEvent? {
        lock.lock(); defer { lock.unlock() }
        return lines.last { $0.string("event") == name }
    }

    /// Waits for the first `name` event.
    func waitEvent(_ name: String, timeout: Double = 30) async throws -> CliEvent {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let e = lastEvent(name) { return e }
            if exitCode != nil, lastEvent(name) == nil {
                throw HarnessError.exit(exitCode, "waiting for \(name)\n\(diagnostics)")
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw HarnessError.timeout("CLI event \(name)\n\(diagnostics)")
    }

    /// The exit status, reported only once both output streams are drained, so no printed event is still in flight.
    /// If a grandchild still holds the pipes `drainGrace` after exit, the streams are force-closed so exit is not hidden.
    var exitCode: Int32? {
        lock.lock(); defer { lock.unlock() }
        guard let code = exited else { return nil }
        if eofStreams < 2 {
            guard let at = exitedAt, Date().timeIntervalSince(at) >= Self.drainGrace else { return nil }
            readers.forEach { $0.readabilityHandler = nil }
            if !buffer.isEmpty { record(line: buffer); buffer = Data() }
            eofStreams = 2
        }
        return code
    }

    func expectExit(_ want: Int32, timeout: Double) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while exitCode == nil, Date() < deadline { try await Task.sleep(nanoseconds: 50_000_000) }
        guard let got = exitCode else { throw HarnessError.timeout("CLI exit within \(timeout) s\n\(diagnostics)") }
        guard got == want else { throw HarnessError.exit(got, diagnostics) }
    }

    var diagnostics: String {
        lock.lock(); defer { lock.unlock() }
        return "stdout:\n\(raw.joined(separator: "\n"))\nstderr:\n\(errText)"
    }

    func terminate() { if process.isRunning { process.terminate() } }

    /// Waits (bounded) until a terminated process has exited, so it can't outlive the test.
    /// Call `terminate()` first; with several processes, terminate them all, then await each.
    func awaitExit(timeout: Double = 5) async {
        let deadline = Date().addingTimeInterval(timeout)
        while exitCode == nil, !Task.isCancelled, Date() < deadline { try? await Task.sleep(nanoseconds: 20_000_000) }
        if process.isRunning { kill(process.processIdentifier, SIGKILL) }
    }
}

/// Polls `condition` every 100 ms until it holds or `timeout` passes. Returns whether it held.
@MainActor
func waitUntil(timeout: Double, poll: UInt64 = 100_000_000, _ condition: @MainActor () -> Bool) async -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if condition() { return true }
        try? await Task.sleep(nanoseconds: poll)
    }
    return condition()
}
