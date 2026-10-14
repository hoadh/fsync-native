import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// Clipboard sync and history through the real bridge and engine, with `fsync-cli` as the other device.
/// The Mac uses a private pasteboard, so the developer's clipboard is never touched.
@MainActor
final class ClipboardIntegrationTests: IntegrationTestCase {
    @MainActor
    private struct Rig {
        let bridge: EngineBridge
        let pb: NSPasteboard
        var sync: ClipboardSync { bridge.clipSync! }
    }

    private func rig(cli: String) async throws -> Rig {
        let pb = scratchPasteboard()
        let bridge = try await pairedMac(cli: cli, pasteboard: pb)
        XCTAssertNotNil(bridge.clipSync)
        let running = await waitUntil(timeout: 10) { bridge.clipSync?.isRunning == true }
        XCTAssertTrue(running, "clipboard sync did not start")
        return Rig(bridge: bridge, pb: pb)
    }

    private func copy(_ text: String, to pb: NSPasteboard, extra: String? = nil) {
        let item = NSPasteboardItem()
        item.setString(text, forType: .string)
        if let extra { item.setData(Data(), forType: NSPasteboard.PasteboardType(extra)) }
        pb.clearContents()
        pb.writeObjects([item])
    }

    private func watch(_ name: String, count: Int = 1) async throws -> CliProcess {
        let w = try spawn(name, ["clip", "watch", "--json", "--count", "\(count)", "--timeout", "60"])
        _ = try await w.waitEvent("ready", timeout: 60)
        return w
    }

    func testCopyOnMacReachesCliAndHistory() async throws {
        let r = try await rig(cli: "k1")
        let watcher = try await watch("k1")
        copy("hello from the mac", to: r.pb)
        let event = try await watcher.waitEvent("clip", timeout: 30)
        XCTAssertEqual(event.string("text"), "hello from the mac")
        let history = try XCTUnwrap(r.bridge.historyModel)
        await history.reload()
        XCTAssertEqual(history.items.first?.preview, "hello from the mac")
        XCTAssertEqual(history.items.first?.outgoing, true)
    }

    /// The history keeps a 200-character preview; copy-back must restore the whole clip from the engine.
    func testHistoryCopyBackRestoresTheFullText() async throws {
        let r = try await rig(cli: "k7")
        let long = (0..<120).map { "line \($0) of a clip longer than the preview" }.joined(separator: "\n")
        copy(long, to: r.pb)
        let history = try XCTUnwrap(r.bridge.historyModel)
        let deadline = Date().addingTimeInterval(30)
        repeat {
            await history.reload()
            if history.items.first?.outgoing == true { break }
            try await Task.sleep(nanoseconds: 200_000_000)
        } while Date() < deadline
        XCTAssertEqual(history.items.first?.outgoing, true, "clip not in history")
        let item = try XCTUnwrap(history.items.first)
        let preview = try XCTUnwrap(item.preview)
        XCTAssertLessThan(preview.count, long.count, "preview must be truncated for this test to mean anything")
        r.pb.clearContents()
        let copied = await r.sync.copyBack(itemId: item.id)
        XCTAssertTrue(copied)
        XCTAssertEqual(r.pb.string(forType: .string), long)
        // Our own write carries the origin marker: it must not come back as a new outgoing item.
        try await Task.sleep(nanoseconds: 2_500_000_000)
        await history.reload()
        XCTAssertEqual(history.items.filter(\.outgoing).count, 1, "copy-back was sent again")
    }

    func testClipFromCliLandsOnPasteboardWithoutEcho() async throws {
        let r = try await rig(cli: "k2")
        let put = try spawn("k2", ["clip", "put", "--text", "hello from the cli", "--wait", "40"])
        let landed = await waitUntil(timeout: 40) { r.pb.string(forType: .string) == "hello from the cli" }
        XCTAssertTrue(landed, "pasteboard: \(r.pb.string(forType: .string) ?? "nil")\n\(put.diagnostics)")
        XCTAssertEqual(r.sync.activity?.direction, .received)
        // The write carries the origin marker, so the Mac does not send it back.
        let history = try XCTUnwrap(r.bridge.historyModel)
        try await Task.sleep(nanoseconds: 2_000_000_000)
        await history.reload()
        XCTAssertEqual(history.items.filter(\.outgoing).count, 0, "echoed: \(history.items.map { $0.preview ?? "" })")
        XCTAssertEqual(history.items.first?.preview, "hello from the cli")
        XCTAssertEqual(history.items.first?.outgoing, false)
    }

    func testConcealedCopyIsNotSentButTheNextOneIs() async throws {
        let r = try await rig(cli: "k3")
        let watcher = try await watch("k3")
        copy("hunter2", to: r.pb, extra: SensitiveFilter.concealedType)
        try await Task.sleep(nanoseconds: 1_500_000_000)
        copy("not a secret", to: r.pb)
        let event = try await watcher.waitEvent("clip", timeout: 30)
        XCTAssertEqual(event.string("text"), "not a secret", "the concealed copy reached the other device")
        let history = try XCTUnwrap(r.bridge.historyModel)
        await history.reload()
        XCTAssertEqual(history.items.compactMap(\.preview), ["not a secret"])
    }

    func testManualModeSendsOnRequestAndHoldsIncoming() async throws {
        let r = try await rig(cli: "k4")
        let settings = try XCTUnwrap(r.bridge.clipSettings)
        await settings.update { $0.clipMode = .manual }
        XCTAssertEqual(r.sync.mode, .manual)

        let watcher = try await watch("k4")
        copy("only when asked", to: r.pb)
        try await Task.sleep(nanoseconds: 2_000_000_000)
        XCTAssertNil(watcher.lastEvent("clip"), "manual mode sent on its own")
        await r.sync.publishCurrent()
        let event = try await watcher.waitEvent("clip", timeout: 30)
        XCTAssertEqual(event.string("text"), "only when asked")
        // One engine per data dir (core `engine.lock`): the watcher must be gone before `clip put`.
        try await watcher.expectExit(0, timeout: 15)

        _ = try spawn("k4", ["clip", "put", "--text", "waiting for you", "--wait", "40"])
        let pending = await waitUntil(timeout: 40) { r.sync.pendingIncoming != nil }
        XCTAssertTrue(pending)
        XCTAssertEqual(r.pb.string(forType: .string), "only when asked", "manual mode overwrote the clipboard")
        await r.sync.acceptPending()
        XCTAssertEqual(r.pb.string(forType: .string), "waiting for you")
    }

    func testHistorySearchPinDeleteAndPaging() async throws {
        let r = try await rig(cli: "k5")
        let engine = try XCTUnwrap(r.bridge.engine)
        func publish(_ text: String) throws -> String {
            let clip = LocalClip(reps: [Rep(mime: "text/plain", data: Data(text.utf8))], files: [], preview: text, sensitive: false)
            return try engine.publishClip(clip: clip).itemId
        }
        let keep = try publish("invoice 2026")
        let drop = try publish("holiday photos")
        let history = HistoryModel(engine: engine, pageSize: 3)  // each publish waits on the offline CLI
        for i in 0..<2 { _ = try publish("filler \(i)") }
        await history.reload()
        XCTAssertEqual(history.items.count, 3)
        XCTAssertTrue(history.hasMore)
        await history.loadMore()
        XCTAssertEqual(history.items.count, 4)
        XCTAssertEqual(Set(history.items.map(\.id)).count, history.items.count, "duplicates across pages")

        await history.reload(query: "invoice")
        XCTAssertEqual(history.items.map(\.id), [keep])
        await history.setPinned(keep, true)
        XCTAssertEqual(history.items.first?.pinned, true)
        await history.reload(query: "")
        await history.delete(drop)
        XCTAssertNil(history.items.first { $0.id == drop })
        await history.clear(keepPinned: true)
        XCTAssertEqual(history.items.map(\.id), [keep])
        XCTAssertNil(history.errorMessage)
    }

    func testSettingsPersistAcrossTheBridge() async throws {
        let r = try await rig(cli: "k6")
        let settings = try XCTUnwrap(r.bridge.clipSettings)
        await settings.update { $0.retentionDays = 5; $0.denylist = ["org.example.vault"] }
        let stored = try XCTUnwrap(r.bridge.engine).settings()
        XCTAssertEqual(stored.retentionDays, 5)
        XCTAssertEqual(stored.denylist, ["org.example.vault"])
        XCTAssertNil(settings.errorMessage)
    }
}
