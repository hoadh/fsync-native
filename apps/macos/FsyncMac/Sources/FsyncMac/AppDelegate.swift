import AppKit
import FsyncMacCore
import FsyncMacUI

@MainActor
enum AppRuntime {
    static let bridge = EngineBridge.live()
    /// Incoming-file notifications; a no-op outside an `.app` bundle.
    static let notifier = IncomingFilesNotifier(bridge: bridge)
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Menubar-only even without an Info.plist (swift run).
        NSApp.setActivationPolicy(.accessory)
        MainActor.assumeIsolated {
            AppRuntime.bridge.boot()
            // Before any notification can be clicked, so a click while the app runs is routed.
            AppRuntime.notifier.start()
        }
        closeLaunchWindow()
    }

    /// Closes the launch window. A `Window` scene opens at launch; `defaultLaunchBehavior(.suppressed)`
    /// would stop that on macOS 15+, but the deployment target is macOS 14 and SceneBuilder cannot switch
    /// on availability, so the window is closed right away on every version. The app reopens it by id
    /// when onboarding needs it (the phase only changes after the async boot).
    /// Looked up by `WindowIDs.main`. Fallback: on the first runloop the view may not have stamped the
    /// identifier yet (it is nil or SwiftUI's scene id), so such a window titled "fsync" is closed too.
    private func closeLaunchWindow() {
        DispatchQueue.main.async {
            for window in NSApp.windows {
                let stamped = window.identifier == WindowIDs.main
                let unstamped = window.identifier == nil || window.identifier?.rawValue == WindowIDs.SceneIDs.main
                if stamped || (unstamped && window.title == "fsync") { window.close() }
            }
        }
    }

    /// Stops the engine off the main thread before quitting (it can block for seconds).
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // First, synchronously: a lazy file entry on the pasteboard cannot be served once we are gone.
        MainActor.assumeIsolated { AppRuntime.bridge.prepareToQuit() }
        Task { @MainActor in
            await AppRuntime.bridge.shutdown()
            NSApp.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}
