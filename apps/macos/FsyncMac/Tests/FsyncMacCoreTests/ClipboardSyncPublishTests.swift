import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Publish ordering and the privacy re-check, against a real (unpaired) engine.
@MainActor
final class ClipboardSyncPublishTests: EngineTestCase {
    private func rig(maxClipBytes: UInt64) throws -> (ClipboardSync, NSPasteboard) {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        var settings = try engine.settings()
        settings.clipMode = .manual
        settings.maxClipBytes = maxClipBytes
        let pb = scratchPasteboard()
        let sync = ClipboardSync(engine: engine, pasteboard: pb, settings: settings)
        sync.start()
        addTeardownBlock { @MainActor in sync.stop() }
        return (sync, pb)
    }

    private var tooLarge: String? { ErrorText.message(for: FsyncError.TooLarge, context: .general) }

    func testStaleChangeCountIsNeverReadOrSent() async throws {
        let (sync, pb) = try rig(maxClipBytes: 16)
        pb.clearContents(); pb.setString(String(repeating: "a", count: 4096), forType: .string)
        await sync.publish(.init(explicit: false, expected: pb.changeCount - 1))
        XCTAssertNil(sync.message, "the filter looked at another change: nothing is read")
        await sync.publish(.init(explicit: false, expected: pb.changeCount))
        XCTAssertEqual(sync.message, tooLarge, "the matching change is read")
    }

    func testPasteboardChangingDuringReadDropsTheOldClip() async throws {
        let (sync, pb) = try rig(maxClipBytes: 16)
        pb.clearContents(); pb.setString(String(repeating: "a", count: 4096), forType: .string)
        async let first: Void = sync.publish(.init(explicit: false, expected: nil))
        pb.clearContents(); pb.setString("b", forType: .string)
        await first
        XCTAssertNotEqual(sync.message, tooLarge, "the large clip was replaced before it could be sent")
    }

    func testSecondCopyDuringPublishIsQueuedNotLost() async throws {
        let (sync, pb) = try rig(maxClipBytes: 16)
        pb.clearContents(); pb.setString(String(repeating: "a", count: 4096), forType: .string)
        async let first: Void = sync.publish(.init(explicit: false, expected: nil))
        pb.clearContents(); pb.setString("ok", forType: .string)
        await sync.publish(.init(explicit: false, expected: nil))
        await first
        XCTAssertTrue(sync.message != nil || sync.activity != nil, "the second clip was attempted (sent or reported)")
        XCTAssertNotEqual(sync.message, tooLarge)
    }
}
