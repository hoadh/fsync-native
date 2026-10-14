// swift-tools-version: 5.9
// Generated bindings (Sources/FsyncCoreKit/fsync_ffi.swift) and FsyncFFI.xcframework are
// produced by scripts/build-xcframework.sh; do not edit them by hand.
import PackageDescription

let package = Package(
    name: "FsyncCoreKit",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "FsyncCoreKit", targets: ["FsyncCoreKit"]),
        .library(name: "TrustResetPrompt", targets: ["TrustResetPrompt"]),
    ],
    targets: [
        // FsyncCoreKit: Rust core bindings.
        .binaryTarget(name: "FsyncFFI", path: "FsyncFFI.xcframework"),
        .target(
            name: "FsyncCoreKit",
            dependencies: ["FsyncFFI"],
            linkerSettings: [
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedLibrary("resolv"),
                .linkedLibrary("c++"),
            ]
        ),
        .testTarget(name: "FsyncCoreKitTests", dependencies: ["FsyncCoreKit"]),

        // TrustResetPrompt: SwiftUI trust-reset prompt over FsyncCoreKit.
        .target(name: "TrustResetPrompt", dependencies: ["FsyncCoreKit"]),
        .testTarget(name: "TrustResetPromptTests", dependencies: ["TrustResetPrompt", "FsyncCoreKit"]),
    ]
)
