import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class ClipboardSyncTests: EngineTestCase {
    private struct Rig {
        let sync: ClipboardSync
        let engine: FsyncEngine
        let pb: NSPasteboard
    }

    private func rig(mode: ClipMode = .auto, frontmost: String? = nil,
                     fetch: RepFetcher? = nil) throws -> Rig {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        var settings = try engine.settings()
        settings.clipMode = mode
        let pb = scratchPasteboard()
        let sync = ClipboardSync(engine: engine, pasteboard: pb, settings: settings, fetchRep: fetch, frontmost: { frontmost })
        sync.start()
        addTeardownBlock { @MainActor in sync.stop() }
        return Rig(sync: sync, engine: engine, pb: pb)
    }

    private func remote(_ text: String) -> ClipItem {
        ClipItem(itemId: UUID().uuidString.lowercased(), from: "peer", fromName: "Peer", kind: .text, tsMs: 1, preview: text,
                 reps: [RepInfo(mime: "text/plain", size: UInt64(text.utf8.count))], files: [], totalSize: UInt64(text.utf8.count), push: false)
    }

    func testReceivedClipIsWrittenInAutoMode() async throws {
        let r = try rig(fetch: { _, _ in Data("from peer".utf8) })
        await r.sync.receive(remote("from peer"))
        XCTAssertEqual(r.pb.string(forType: .string), "from peer")
        XCTAssertEqual(r.sync.activity?.direction, .received)
    }

    func testCopyBackWritesPreview() async throws {
        let r = try rig()
        let copied = await r.sync.copyBack(text: "from history")
        XCTAssertTrue(copied)
        XCTAssertEqual(r.pb.string(forType: .string), "from history")
    }

    /// Both refusals happen before the engine is called, so the message is the app's own.
    func testExplicitSendRefusesConcealedClip() async throws {
        let r = try rig(mode: .manual)
        let item = NSPasteboardItem()
        item.setString("hunter2", forType: .string)
        item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.concealedType))
        r.pb.clearContents(); r.pb.writeObjects([item])
        await r.sync.publishCurrent()
        XCTAssertEqual(r.sync.message, SkipReason.concealed.userText)
        XCTAssertNil(r.sync.activity)
    }

    func testClipOverTheSizeLimitIsRefused() async throws {
        let r = try rig(mode: .manual)
        var s = try r.engine.settings()
        s.maxClipBytes = 16
        r.sync.apply(settings: s)
        r.pb.clearContents(); r.pb.setString(String(repeating: "a", count: 4096), forType: .string)
        await r.sync.publishCurrent()
        XCTAssertEqual(r.sync.message, ErrorText.message(for: FsyncError.TooLarge, context: .general))
        XCTAssertNil(r.sync.activity)
    }

    func testManualModeKeepsNewestUntilAccepted() async throws {
        let r = try rig(mode: .manual, fetch: { _, idx in Data("second".utf8) })
        await r.sync.receive(remote("first"))
        let second = remote("second")
        await r.sync.receive(second)
        XCTAssertEqual(r.sync.pendingIncoming?.itemId, second.itemId)
        XCTAssertNil(r.pb.string(forType: .string))
        await r.sync.acceptPending()
        XCTAssertEqual(r.pb.string(forType: .string), "second")
        XCTAssertNil(r.sync.pendingIncoming)
    }

    func testFailedFetchReportsAndKeepsPasteboard() async throws {
        let r = try rig(fetch: { _, _ in throw FsyncError.PeerOffline })
        r.pb.clearContents(); r.pb.setString("mine", forType: .string)
        try await Task.sleep(nanoseconds: 300_000_000)
        await r.sync.receive(remote("lost"))
        XCTAssertEqual(r.pb.string(forType: .string), "mine")
        XCTAssertEqual(r.sync.message, "Couldn't receive the clip from Peer.")
    }

    func testStoppedSyncIgnoresIncoming() async throws {
        let r = try rig(fetch: { _, _ in Data("x".utf8) })
        r.sync.stop()
        await r.sync.receive(remote("x"))
        XCTAssertNil(r.pb.string(forType: .string))
    }

}
