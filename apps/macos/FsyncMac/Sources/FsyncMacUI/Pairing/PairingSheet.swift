import FsyncMacCore
import SwiftUI

/// The pairing flow as a sheet over the main window (this Mac is trusted and adds a device).
/// The sheet closes itself once the flow ends: cancelled, failed and dismissed, or done and dismissed.
public struct PairingSheet: View {
    @ObservedObject private var model: PairingModel
    private let onClose: () -> Void

    public init(model: PairingModel, onClose: @escaping () -> Void) {
        self.model = model
        self.onClose = onClose
    }

    public var body: some View {
        PairingFlowView(model: model, role: .join)
            .frame(width: 420)
            .onAppear { model.openJoin() }
            // Esc on the success screen: `cancel()` keeps it, so clear it here (dismiss ignores a running flow).
            .onDisappear { model.dismiss() }
            // Closing by any route (Esc included) ends the flow: PairingFlowView cancels on disappear.
            .onChange(of: model.isActive) { _, active in if !active { onClose() } }
    }
}
