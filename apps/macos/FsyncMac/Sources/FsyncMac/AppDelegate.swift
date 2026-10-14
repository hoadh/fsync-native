import AppKit
import FsyncMacCore

@MainActor
enum AppRuntime {
    static let bridge = EngineBridge.live()
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Menubar-only even without an Info.plist (swift run).
        NSApp.setActivationPolicy(.accessory)
        MainActor.assumeIsolated { AppRuntime.bridge.boot() }
        closeLaunchWindow()
    }

    /// A `Window` scene opens at launch. `defaultLaunchBehavior(.suppressed)` would stop that on
    /// macOS 15+, but SceneBuilder cannot switch on availability with the macOS 13 deployment target,
    /// so the window is closed right away on every version. The app reopens it by id when onboarding
    /// needs it (the phase only changes after the async boot).
    private func closeLaunchWindow() {
        DispatchQueue.main.async {
            NSApp.windows.filter { $0.title == "fsync" }.forEach { $0.close() }
        }
    }

    /// Stops the engine off the main thread before quitting (it can block for seconds).
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        Task { @MainActor in
            await AppRuntime.bridge.shutdown()
            NSApp.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}
