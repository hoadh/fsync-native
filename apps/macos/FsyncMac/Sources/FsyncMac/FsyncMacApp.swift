import FsyncMacCore
import FsyncMacUI
import SwiftUI

@main
struct FsyncMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var bridge = AppRuntime.bridge
    @Environment(\.openWindow) private var openWindow
    /// Onboarding opens the window by itself once, so nobody has to find the menubar icon first.
    @State private var autoOpened = false

    var body: some Scene {
        MenuBarExtra {
            MenuPopover(bridge: bridge, openMain: showMain, openSettings: showSettings, openHistory: showHistory)
        } label: {
            Image(systemName: bridge.anyPeerOnline ? "doc.on.clipboard.fill" : "doc.on.clipboard")
                .onReceive(bridge.$phase) { phase in
                    guard !autoOpened else { return }
                    switch phase {
                    case .signedOut, .needsPairing, .bootFailed:
                        autoOpened = true
                        // `$phase` fires in willSet: opening now would render the old phase and the
                        // window would miss this change. Open on the next turn, once the value is
                        // stored, unless it was already replaced by `.ready` (no window needed).
                        // Not `$phase.receive(on:)`: this label re-subscribes on every body update,
                        // which drops a queued delivery, so the window would never open.
                        DispatchQueue.main.async {
                            if bridge.phase != .ready { showMain() }
                        }
                    case .ready: autoOpened = true  // a returning user needs no window
                    case .booting: break
                    }
                }
                .onReceive(bridge.windowRequest) { showMain() }
        }
        .menuBarExtraStyle(.window)

        Window("fsync", id: "main") {
            RootView(bridge: bridge)
        }

        Window("Clipboard history", id: "history") {
            if let history = bridge.historyModel, bridge.phase == .ready {
                HistoryView(history: history, sync: bridge.clipSync)
            } else {
                Text("Sign in to see your history.").padding(40)
            }
        }

        Settings { SettingsView(bridge: bridge) }
    }

    private func showSettings() {
        NSApp.activate(ignoringOtherApps: true)
        // SettingsLink needs macOS 14 and a view hierarchy; the responder-chain action works from a menu on both.
        let name = if #available(macOS 14, *) { "showSettingsWindow:" } else { "showPreferencesWindow:" }
        NSApp.sendAction(Selector(name), to: nil, from: nil)
    }

    private func showHistory() {
        openWindow(id: "history")
        NSApp.activate(ignoringOtherApps: true)
    }

    private func showMain() {
        openWindow(id: "main")
        NSApp.activate(ignoringOtherApps: true)
        // An accessory app's new window can stay behind other apps; bring it forward explicitly.
        DispatchQueue.main.async {
            NSApp.windows.first { $0.title == "fsync" && $0.canBecomeMain }?.makeKeyAndOrderFront(nil)
        }
    }
}
