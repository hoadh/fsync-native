import FsyncMacCore
import SwiftUI

/// Settings scene: General, then Sync, Privacy and Storage once this Mac is ready, then Account.
/// Every pane is a grouped `Form` that saves on change.
public struct SettingsView: View {
    @ObservedObject private var bridge: EngineBridge
    @StateObject private var launch = LaunchAtLogin()

    public init(bridge: EngineBridge) { self.bridge = bridge }

    public var body: some View {
        TabView {
            GeneralSettingsView(launch: launch)
                .tabItem { Label("General", systemImage: "gearshape") }
            if let clip = bridge.clipSettings, bridge.phase == .ready {
                SyncSettingsView(settings: clip).tabItem { Label("Sync", systemImage: "arrow.triangle.2.circlepath") }
                PrivacySettingsView(settings: clip).tabItem { Label("Privacy", systemImage: "hand.raised") }
                StorageSettingsView(settings: clip).tabItem { Label("Storage", systemImage: "internaldrive") }
            }
            AccountSettingsView(bridge: bridge)
                .tabItem { Label("Account", systemImage: "person.crop.circle") }
        }
        .frame(width: 500)
        .frame(minHeight: 380)
    }
}
