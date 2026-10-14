import Foundation

/// Collects check results; `--out FILE` also writes the log (type names and sizes only, never content).
final class Report {
    private(set) var failures = 0
    private(set) var open = 0
    private var lines: [String] = []

    func line(_ text: String) { print(text); lines.append(text) }

    func check(_ name: String, _ ok: Bool, _ detail: @autoclosure () -> String = "") {
        if !ok { failures += 1 }
        line("[\(ok ? "PASS" : "FAIL")] \(name)\(detail().isEmpty ? "" : " - " + detail())")
    }

    /// Facts that are recorded, not judged.
    func info(_ name: String, _ detail: String) { line("[INFO] \(name) - \(detail)") }

    /// Needs a human or hardware the automation cannot drive.
    func manual(_ name: String, _ why: String) { open += 1; line("[OPEN] \(name) - \(why)") }

    func finish(to path: String?) -> Int32 {
        line("\nsummary: failures=\(failures) open(manual)=\(open)")
        if let path { try? lines.joined(separator: "\n").appending("\n").write(toFile: path, atomically: true, encoding: .utf8) }
        return failures == 0 ? 0 : 1
    }
}

func fmt(_ x: Double) -> String { String(format: "%.2f", x) }
