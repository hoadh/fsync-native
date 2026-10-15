import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// New history items must reach an open (or kept-alive) history window: every path that adds a row
/// asks the history model to reload. Before this, the list loaded once and stayed empty.
@MainActor
final class EngineBridgeHistoryRefreshTests: EngineTestCase {
    private func readyBridge() async throws -> (EngineBridge, HistoryModel) {
        let secrets = AppSecrets(store: makeStore())
        let bridge = EngineBridge(config: offlineConfig, secrets: secrets, dataDir: try tempDir(), retryDelays: [60])
        bridge.boot()
        await eventually("engine ready") { bridge.phase == .signedOut }
        bridge.applyAccountState(.ready)
        let history = try XCTUnwrap(bridge.historyModel)
        return (bridge, history)
    }

    private func clip(_ kind: ClipKind) -> ClipItem {
        ClipItem(itemId: "0a1b", from: "node", fromName: "Peer", kind: kind, tsMs: 1, preview: "p", reps: [],
                 files: kind == .files ? [FileInfo(fileId: 0, name: "a.txt", relPath: "a.txt", size: 1)] : [],
                 totalSize: 1, push: true)
    }

    private func expectReload(_ history: HistoryModel, after action: () -> Void) async {
        await eventually("settled") { !history.isLoading }
        let before = history.generation
        action()
        await eventually("history reloaded") { history.generation > before }
    }

    func testIncomingFilesReloadsHistory() async throws {
        let (bridge, history) = try await readyBridge()
        await expectReload(history) { bridge.receive(.incomingFiles(clip(.files))) }
        await bridge.shutdown()
    }

    func testRemoteClipReloadsHistory() async throws {
        let (bridge, history) = try await readyBridge()
        await expectReload(history) { bridge.receive(.remoteClip(clip(.text))) }
        await bridge.shutdown()
    }

    func testClipActivityReloadsHistory() async throws {
        let (bridge, history) = try await readyBridge()
        await expectReload(history) { bridge.clipSync?.activity = SyncActivity(direction: .sent, summary: "x", at: Date()) }
        await bridge.shutdown()
    }

    func testFilesSentReloadsHistory() async throws {
        let (bridge, history) = try await readyBridge()
        await expectReload(history) { bridge.filesSync?.onSent?() }
        await bridge.shutdown()
    }
}
