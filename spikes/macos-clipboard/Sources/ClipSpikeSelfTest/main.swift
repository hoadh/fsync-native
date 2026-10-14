import AppKit
import ClipSpikeCore
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
if args.first == "--child" { Child.run(Array(args.dropFirst())) }

func option(_ name: String) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    return args[i + 1]
}

NSApplication.shared.setActivationPolicy(.accessory)
NSApplication.shared.finishLaunching()
let report = Report()
report.line("ClipSpike self-test, macOS \(ProcessInfo.processInfo.operatingSystemVersionString), \(Date())")
let cpu = Double(option("--cpu-seconds") ?? "") ?? 20

if args.contains("--offer-promise") {
    Offer.promise(pasteboard: option("--pb").map { NSPasteboard(name: NSPasteboard.Name($0)) } ?? .general)
}
if args.contains("--offer") {
    // Manual mode: offer lazy files on the general pasteboard and wait for a human to paste.
    Offer.run(size: Int(option("--size-mib") ?? "") ?? 200, delay: Double(option("--delay") ?? "") ?? 3)
}
WatcherChecks.run(report, cpuSeconds: cpu)
LazyChecks.run(report, long: args.contains("--long"))
if args.contains("--general") { GeneralChecks.run(report) }
exit(report.finish(to: option("--out")))
