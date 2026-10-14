import Carbon.HIToolbox
import XCTest
@testable import FsyncMacCore

@MainActor
final class HotkeyManagerTests: XCTestCase {
    private let key = UInt32(kVK_F19), mods = UInt32(controlKey | optionKey)

    func testRegisterUnregisterAndReleaseOnDeinit() {
        var first: HotkeyManager? = HotkeyManager(keyCode: key, modifiers: mods) {}
        XCTAssertEqual(first?.register(), true)
        let second = HotkeyManager(keyCode: key, modifiers: mods) {}
        XCTAssertFalse(second.register(), "the combination is taken")
        first = nil
        XCTAssertTrue(second.register(), "dropping the owner released the key")
        second.unregister()
        XCTAssertFalse(second.isRegistered)
    }
}
