import AppKit
import Foundation

struct ChildResult {
    var json: [String: Any]?
    var seconds: Double
    var timedOut: Bool
    func int(_ key: String) -> Int? { json?[key] as? Int }
    var files: [[String: Any]] { (json?["files"] as? [[String: Any]]) ?? [] }
}

/// Pumps the main run loop while `condition` holds, so lazy providers owned by this process can
/// answer a reader in another process.
func pump(until deadline: TimeInterval, while condition: () -> Bool) {
    let end = Date().addingTimeInterval(deadline)
    while condition() && Date() < end { RunLoop.current.run(until: Date().addingTimeInterval(0.01)) }
}

func startChild(_ args: [String], stdin: Pipe? = nil) throws -> (Process, Pipe) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: Bundle.main.executablePath ?? CommandLine.arguments[0])
    p.arguments = ["--child"] + args
    let out = Pipe()
    p.standardOutput = out
    if let stdin { p.standardInput = stdin }
    try p.run()
    return (p, out)
}

func runChild(_ args: [String], timeout: TimeInterval = 180) -> ChildResult {
    let started = Date()
    guard let (proc, out) = try? startChild(args) else { return ChildResult(json: nil, seconds: 0, timedOut: false) }
    pump(until: timeout) { proc.isRunning }
    let timedOut = proc.isRunning
    if timedOut { proc.terminate() }
    let data = out.fileHandleForReading.readDataToEndOfFile()
    let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    return ChildResult(json: json, seconds: Date().timeIntervalSince(started), timedOut: timedOut)
}

func uniquePasteboard() -> NSPasteboard { NSPasteboard(name: NSPasteboard.Name("com.fsync.spike.\(UUID().uuidString)")) }

func tempDir(_ tag: String) -> URL {
    let d = FileManager.default.temporaryDirectory.appendingPathComponent("clipspike-\(tag)-\(UUID().uuidString)")
    try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
    return d
}

func cpuSeconds() -> Double {
    var u = rusage()
    getrusage(RUSAGE_SELF, &u)
    func s(_ t: timeval) -> Double { Double(t.tv_sec) + Double(t.tv_usec) / 1e6 }
    return s(u.ru_utime) + s(u.ru_stime)
}
