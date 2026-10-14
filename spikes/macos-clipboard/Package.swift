// swift-tools-version: 6.0
// Spike for phase 04: macOS pasteboard watching, sensitive-type filtering, lazy file paste.
// Run: swift run -c release ClipSpikeSelfTest [--general] [--cpu-seconds N]
import PackageDescription

let package = Package(
    name: "ClipSpike",
    platforms: [.macOS(.v13)],
    targets: [
        .target(name: "ClipSpikeCore", swiftSettings: [.swiftLanguageMode(.v5)]),
        .executableTarget(name: "ClipSpikeSelfTest", dependencies: ["ClipSpikeCore"],
                          swiftSettings: [.swiftLanguageMode(.v5)]),
    ]
)
