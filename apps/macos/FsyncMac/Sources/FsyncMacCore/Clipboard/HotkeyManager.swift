import Carbon.HIToolbox
import Foundation

/// One global hotkey (Carbon `RegisterEventHotKey`: needs no Accessibility permission).
@MainActor
public final class HotkeyManager {
    /// Control + Option + C: "send the clipboard now" in manual mode.
    public static let sendClipboard = (keyCode: UInt32(kVK_ANSI_C), modifiers: UInt32(controlKey | optionKey))
    public static let sendClipboardLabel = "⌃⌥C"

    // Carbon refs are plain pointers; nonisolated(unsafe) lets deinit release them from any thread.
    nonisolated(unsafe) private var hotKey: EventHotKeyRef?
    nonisolated(unsafe) private var handler: EventHandlerRef?
    private let keyCode: UInt32
    private let modifiers: UInt32
    private let action: @MainActor () -> Void

    public init(keyCode: UInt32 = sendClipboard.keyCode, modifiers: UInt32 = sendClipboard.modifiers,
                action: @escaping @MainActor () -> Void) {
        self.keyCode = keyCode
        self.modifiers = modifiers
        self.action = action
    }

    public var isRegistered: Bool { hotKey != nil }

    /// False when another app already owns the combination (or the call failed).
    @discardableResult
    public func register() -> Bool {
        guard hotKey == nil else { return true }
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let box = Unmanaged.passUnretained(self).toOpaque()
        let status = InstallEventHandler(GetApplicationEventTarget(), { _, _, user in
            guard let user else { return OSStatus(eventNotHandledErr) }
            let me = Unmanaged<HotkeyManager>.fromOpaque(user).takeUnretainedValue()
            DispatchQueue.main.async { MainActor.assumeIsolated { me.fire() } }
            return noErr
        }, 1, &spec, box, &handler)
        guard status == noErr else { return false }
        let id = EventHotKeyID(signature: OSType(0x6673_796E), id: 1)  // 'fsyn'
        guard RegisterEventHotKey(keyCode, modifiers, id, GetApplicationEventTarget(), 0, &hotKey) == noErr else {
            if let handler { RemoveEventHandler(handler) }
            handler = nil
            return false
        }
        return true
    }

    public func unregister() { release() }

    deinit { release() }  // the Carbon handler holds an unretained pointer to self

    nonisolated private func release() {
        if let hotKey { UnregisterEventHotKey(hotKey) }
        if let handler { RemoveEventHandler(handler) }
        hotKey = nil
        handler = nil
    }

    func fire() { action() }
}
