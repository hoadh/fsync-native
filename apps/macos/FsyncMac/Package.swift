// swift-tools-version: 6.0
import PackageDescription

let coreKit = Target.Dependency.product(name: "FsyncCoreKit", package: "FsyncCoreKit")
let trustPrompt = Target.Dependency.product(name: "TrustResetPrompt", package: "FsyncCoreKit")

let package = Package(
    name: "FsyncMac",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "FsyncMac", targets: ["FsyncMac"])],
    dependencies: [.package(path: "../Packages/FsyncCoreKit")],
    targets: [
        // Logic: engine bridge, Keychain, models. No SwiftUI.
        .target(name: "FsyncMacCore", dependencies: [coreKit, trustPrompt]),
        // SwiftUI views over the models.
        .target(name: "FsyncMacUI", dependencies: ["FsyncMacCore", coreKit, trustPrompt]),
        // @main only.
        .executableTarget(name: "FsyncMac", dependencies: ["FsyncMacUI", "FsyncMacCore"]),
        .testTarget(name: "FsyncMacCoreTests", dependencies: ["FsyncMacCore", coreKit, trustPrompt]),
        .testTarget(name: "FsyncMacUITests", dependencies: ["FsyncMacUI", "FsyncMacCore", coreKit, trustPrompt]),
    ]
)
