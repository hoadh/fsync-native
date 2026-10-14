import SwiftUI

/// The one sign-out control for the popover, the main window and Settings: it asks first, and
/// Cancel is the default answer (Return), so a stray key press cannot sign the user out.
struct SignOutButton: View {
    var title = "Sign out…"
    var isDisabled = false
    let action: () -> Void
    @State private var confirming = false

    var body: some View {
        Button(title) { confirming = true }
            .disabled(isDisabled)
            .confirmationDialog("Sign out of fsync?", isPresented: $confirming) {
                Button("Cancel", role: .cancel) {}.keyboardShortcut(.defaultAction)
                Button("Sign out", role: .destructive, action: action)
            } message: {
                Text("This Mac stops syncing until you sign in again.")
            }
    }
}
