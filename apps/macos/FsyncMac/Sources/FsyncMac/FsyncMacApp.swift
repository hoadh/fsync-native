import FsyncMacCore
import FsyncMacUI
import AppKit
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
            MenuPopover(
                bridge: bridge, openMain: showMain, openSettings: showSettings, openHistory: showHistory,
                openTransfers: showTransfers, addDevice: addDevice)
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

        // Declared before `Settings`: it owns `openSettings` (see SettingsOpener.swift). Never visible.
        Window("", id: WindowIDs.SceneIDs.settingsOpener) { SettingsOpenerView() }
            .windowStyle(.hiddenTitleBar)
            .defaultSize(width: 1, height: 1)

        Window("fsync", id: WindowIDs.SceneIDs.main) {
            RootView(bridge: bridge)
                .background(WindowIdentifier(WindowIDs.main))
        }
        .defaultSize(width: 760, height: 520)
        .windowResizability(.contentMinSize)

        Settings { SettingsView(bridge: bridge) }
    }

    private func showSettings() {
        // The opener window may not exist yet (a Window scene opens lazily); creating it is harmless, it hides itself.
        if !NSApp.windows.contains(where: WindowIDs.isSettingsOpener) { openWindow(id: WindowIDs.SceneIDs.settingsOpener) }
        SettingsRequest.send()
    }

    private func showHistory() {
        bridge.navigation.show(.history)  // the bridge refreshes the list on this assignment
        showMain()
    }

    private func showTransfers() {
        bridge.navigation.show(.transfers)
        showMain()
    }

    private func addDevice() {
        bridge.navigation.presentPairing()
        showMain()
    }

    private func showMain() {
        openWindow(id: WindowIDs.SceneIDs.main)
        NSApp.activate()
        // An accessory app's new window can stay behind other apps; bring it forward explicitly. The
        // identifier is set when the window's view appears, so retry once on the next turn if it is not there yet.
        func front() -> Bool {
            guard let window = WindowIDs.window(WindowIDs.main) else { return false }
            window.makeKeyAndOrderFront(nil)
            return true
        }
        DispatchQueue.main.async { if !front() { DispatchQueue.main.async { _ = front() } } }
    }
}
