import AppKit
import FsyncCoreKit
import SwiftUI
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

/// Engine delegate that ignores everything; the render tests only need a real engine object.
final class QuietDelegate: EngineDelegate, @unchecked Sendable {
    func onRemoteClip(item: ClipItem) {}
    func onDeviceStatus(device: Device) {}
    func onTransfer(event: TransferEvent) {}
    func onIncomingFiles(offer: ClipItem) {}
    func onPairingEvent(event: PairingEvent) {}
    func onAccountEvent(event: AccountEvent) {}
    func onSessionChanged(tokens: SessionTokens) {}
    func onError(error: FsyncError) {}
}

/// Fixtures and rendering helpers shared by the UI render tests.
@MainActor
enum RenderSupport {
    /// `ImageRenderer` render. It draws `List`, `Table` and `NavigationSplitView` blank: use `hostOffscreen` for those.
    static func render<V: View>(_ view: V, _ name: String) throws {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        let image = try XCTUnwrap(renderer.nsImage, name)
        XCTAssertGreaterThan(image.size.width, 100, name)
        XCTAssertGreaterThan(image.size.height, 50, name)
    }

    /// Hosts `view` in an offscreen (never shown) window at an explicit size, lays it out and draws it once.
    /// `List`/`Table` have no intrinsic size (fittingSize is zero), so the view gets a fixed frame. The check is that
    /// drawing produced more than one distinct pixel colour (not blank).
    @discardableResult
    static func hostOffscreen<V: View>(_ view: V, _ name: String, size: CGSize = CGSize(width: 760, height: 520)) throws -> NSHostingView<some View> {
        let host = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
        let window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: true)
        window.contentView = host
        host.frame = CGRect(origin: .zero, size: size)
        host.layoutSubtreeIfNeeded()
        let rep = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds), name)
        host.cacheDisplay(in: host.bounds, to: rep)
        var colours = Set<[CGFloat]>()
        for x in stride(from: 0, to: rep.pixelsWide, by: 6) {
            for y in stride(from: 0, to: rep.pixelsHigh, by: 6) {
                if let c = rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) {
                    colours.insert([c.redComponent, c.greenComponent, c.blueComponent, c.alphaComponent])
                }
            }
        }
        XCTAssertGreaterThan(colours.count, 1, "\(name): drew a blank view")
        return host
    }

    /// A real engine in a temp dir (no network); the directory is removed after the test.
    static func makeEngine(for test: XCTestCase) throws -> FsyncEngine {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        test.addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let cfg = EngineConfig(
            dataDir: dir.path, apiBase: "http://127.0.0.1:9", relayUrl: nil, deviceName: "t", platform: .mac,
            deviceSecret: Data(repeating: 7, count: 32), historyKey: Data(repeating: 9, count: 32),
            forceRelay: false, allowInsecureHttp: true)
        return try FsyncEngine(cfg: cfg, delegate: QuietDelegate())
    }

    /// This Mac plus one connected and one offline device.
    static func devices(_ engine: FsyncEngine, others: Bool = true) -> DevicesModel {
        let model = DevicesModel(engine: engine, onSignInRequired: {})
        model.merge(Device(nodeId: "s", name: "This Mac", platform: .mac, status: .offline, online: false, lastSeenMs: 0, isSelf: true))
        guard others else { return model }
        model.merge(Device(nodeId: "a", name: "Work PC", platform: .win, status: .lan, online: true, lastSeenMs: 0, isSelf: false))
        model.merge(Device(nodeId: "b", name: "CLI", platform: .cli, status: .offline, online: false, lastSeenMs: 1_700_000_000_000, isSelf: false))
        return model
    }

    static func files(_ engine: FsyncEngine, transfers: TransfersModel) -> FilesSync {
        let files = FilesSync(engine: engine, fetcher: FileFetchService(engine: engine, transfers: transfers)) { false }
        files.start()
        return files
    }

    static func offer(id: String = "0a", from: String = "Phone") -> ClipItem {
        ClipItem(itemId: id, from: "n", fromName: from, kind: .files, tsMs: 1, preview: nil, reps: [],
                 files: [FileInfo(fileId: 0, name: "report.pdf", relPath: "report.pdf", size: 5)], totalSize: 5, push: true)
    }
}
