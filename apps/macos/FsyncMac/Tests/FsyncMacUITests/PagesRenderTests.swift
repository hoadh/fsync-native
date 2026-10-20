import AppKit
import FsyncCoreKit
import SwiftUI
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

/// Each page empty and populated. List/Table pages are hosted in an offscreen window (ImageRenderer draws them blank).
@MainActor
final class PagesRenderTests: XCTestCase {
    func testDevicesPageEmptyAndPopulated() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let transfers = TransfersModel()
        let files = RenderSupport.files(engine, transfers: transfers)
        try RenderSupport.hostOffscreen(
            DevicesPageView(devices: RenderSupport.devices(engine, others: false), files: files, onPair: {}), "devices empty")
        try RenderSupport.hostOffscreen(
            DevicesPageView(devices: RenderSupport.devices(engine), files: files, onPair: {}), "devices populated")
    }

    func testDropValidationMessagesAreSpecific() {
        XCTAssertEqual(DevicesPageLogic.text(for: .empty), "Nothing to send.")
        XCTAssertTrue(DevicesPageLogic.text(for: .notFileURL).contains("from this Mac"))
        XCTAssertEqual(DevicesPageLogic.text(for: .missing(name: "a.txt")), "“a.txt” no longer exists.")
    }

    private func tempFiles() throws -> (file: URL, folder: URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sub"), withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("a.txt")
        try Data(repeating: 1, count: 2048).write(to: file)
        return (file, dir.appendingPathComponent("sub"))
    }

    private func device(_ id: String, online: Bool = true, isSelf: Bool = false, name: String = "Work PC") -> Device {
        Device(nodeId: id, name: name, platform: .win, status: online ? .lan : .offline, online: online, lastSeenMs: 0, isSelf: isSelf)
    }

    func testDropOnOnlineDeviceAsksForConfirmationListingEveryItem() throws {
        let (file, folder) = try tempFiles()
        guard case .confirm(let request) = DevicesPageLogic.decide(urls: [file, folder, file], on: device("a")) else {
            return XCTFail("expected a confirmation")
        }
        XCTAssertEqual(request.nodeId, "a")
        XCTAssertEqual(request.deviceName, "Work PC")
        XCTAssertEqual(request.items.map(\.name), ["a.txt", "sub"], "duplicates dropped, order kept")
        XCTAssertEqual(request.items[1].sizeText, "Folder")
        XCTAssertTrue(request.items[1].isDirectory)
        XCTAssertFalse(request.items[0].isDirectory)
        XCTAssertNotEqual(request.items[0].sizeText, "Folder")
        try RenderSupport.hostOffscreen(
            DropSendConfirmation(request: request, onSend: {}, onCancel: {}), "drop confirmation", size: CGSize(width: 420, height: 320))
    }

    func testDropIsRejectedForOfflineSelfMissingAndNonFileInput() throws {
        let (file, _) = try tempFiles()
        for target in [device("a", online: false), device("s", isSelf: true)] {
            guard case .reject = DevicesPageLogic.decide(urls: [file], on: target) else { return XCTFail("\(target.nodeId)") }
        }
        XCTAssertEqual(DevicesPageLogic.decide(urls: [], on: device("a")), .reject("Nothing to send."))
        XCTAssertEqual(DevicesPageLogic.decide(urls: [URL(string: "https://example.com/x")!], on: device("a")),
                       .reject(DevicesPageLogic.text(for: .notFileURL)))
        let gone = file.deletingLastPathComponent().appendingPathComponent("gone.txt")
        XCTAssertEqual(DevicesPageLogic.decide(urls: [gone], on: device("a")), .reject("“gone.txt” no longer exists."))
    }

    func testDropHighlightAndDialogsFollowTheDeviceList() {
        XCTAssertTrue(DevicesPageLogic.acceptsDrop(device("a")))
        XCTAssertFalse(DevicesPageLogic.acceptsDrop(device("a", online: false)))
        XCTAssertFalse(DevicesPageLogic.acceptsDrop(device("s", isSelf: true)))
        let list = [device("a"), device("b")]
        XCTAssertTrue(DevicesPageLogic.isPresented(nodeId: "a", in: list))
        XCTAssertFalse(DevicesPageLogic.isPresented(nodeId: "c", in: list), "device removed while the dialog was open")
        XCTAssertFalse(DevicesPageLogic.isPresented(nodeId: nil, in: list))
        XCTAssertEqual(DevicesPageLogic.displayName(device("a", name: "\u{202E}\n")), "another device")
        XCTAssertEqual(DevicesPageLogic.displayName(device("a", name: "A\nB")), "AB")
    }

    func testHistoryPageEmptyAndRows() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let devices = RenderSupport.devices(engine)
        let history = HistoryModel(engine: engine)
        try RenderSupport.hostOffscreen(HistoryView(history: history, devices: devices, sync: nil), "history empty")
        // The page needs a started engine to hold items, so the populated rows are hosted directly.
        let items = [
            HistoryItem(id: "1", outgoing: true, peer: nil, kind: .text, createdMs: 1_700_000_000_000, totalSize: 13, preview: "hello history", pinned: true),
            HistoryItem(id: "2", outgoing: false, peer: "a", kind: .files, createdMs: 1_700_000_100_000, totalSize: 5000, preview: nil, pinned: false),
        ]
        let list = List(items, id: \.id) { item in
            HistoryRowView(item: item, peerName: PeerText.deviceName(peer: item.peer, in: devices.devices),
                           onCopy: item.canCopyBack ? {} : nil, onPin: {}, onDelete: {})
        }
        try RenderSupport.hostOffscreen(list, "history rows")
    }

    func testTransfersPageEmptyAndPopulated() async throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let transfers = TransfersModel()
        let files = RenderSupport.files(engine, transfers: transfers)
        let devices = RenderSupport.devices(engine)
        try RenderSupport.hostOffscreen(TransfersPageView(transfers: transfers, files: files, devices: devices), "transfers empty")

        let done = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).txt")
        try Data("x".utf8).write(to: done)
        addTeardownBlock { try? FileManager.default.removeItem(at: done) }
        func event(_ id: UInt64, _ state: TransferState, direction: TransferDirection = .receive, path: String? = nil, message: String? = nil) -> TransferEvent {
            TransferEvent(transferId: id, itemId: "i\(id)", fileId: 0, direction: direction, state: state,
                          bytesDone: 50, bytesTotal: 100, path: path, message: message)
        }
        transfers.handle(event(1, .progress, direction: .send))
        transfers.handle(event(2, .failed, message: "connection lost"))
        transfers.handle(event(3, .done, path: done.path))
        transfers.setName("big.zip", for: 1)
        transfers.setName("photo.png", for: 2)
        transfers.setName("notes.txt", for: 3)
        await files.receive(offer: RenderSupport.offer())
        XCTAssertEqual(files.offers.count, 1)
        XCTAssertEqual(transfers.rows.count, 3)
        XCTAssertTrue(transfers.rows.first { $0.id == 1 }?.isActive == true)
        XCTAssertNotNil(transfers.rows.first { $0.id == 3 }?.revealURL)
        // No retry spec exists for the failed row, so Retry renders disabled.
        XCTAssertFalse(files.canRetry(try XCTUnwrap(transfers.rows.first { $0.id == 2 })))
        try RenderSupport.hostOffscreen(TransfersPageView(transfers: transfers, files: files, devices: devices), "transfers populated")
        try RenderSupport.render(TransferOfferSection(files: files).padding(20).frame(width: 520), "offer section")
    }

    /// A failed receive whose retry spec is known and whose sender is trusted: Retry is enabled (canRetry true).
    func testFailedRowWithRetrySpecCanRetry() async throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let transfers = TransfersModel()
        nonisolated(unsafe) let model = transfers
        let fetcher = FileFetchService(transfers: transfers) { _, _, _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    model.handle(TransferEvent(transferId: 7, itemId: "0a", fileId: 0, direction: .receive, state: .failed,
                                               bytesDone: 0, bytesTotal: 5, path: nil, message: "peer went away"))
                }
            }
            return 7
        }
        let files = FilesSync(engine: engine, fetcher: fetcher, downloads: FileManager.default.temporaryDirectory) { false }
        files.isPeerTrusted = { $0 == "n" }
        files.start()
        await files.receive(offer: RenderSupport.offer())
        await files.accept(try XCTUnwrap(files.offers.first))
        let row = try XCTUnwrap(transfers.rows.first)
        XCTAssertEqual(row.state, .failed)
        XCTAssertTrue(files.canRetry(row))
        XCTAssertEqual(TransfersTableView.failureText(row), "peer went away")
        try RenderSupport.hostOffscreen(TransfersPageView(transfers: transfers, files: files, devices: RenderSupport.devices(engine)), "transfers retryable")
        files.isPeerTrusted = { _ in false }
        XCTAssertFalse(files.canRetry(row), "sender no longer trusted")
    }

    func testTransferNameAndStatusText() {
        let row = TransferRow(id: 1, itemId: "i", direction: .receive, state: .failed, bytesDone: 0, bytesTotal: 0, path: nil, message: nil, name: "\u{202E}a.txt")
        XCTAssertEqual(TransfersTableView.name(row), "a.txt")
        var unnamed = row
        unnamed.name = nil
        XCTAssertEqual(TransfersTableView.name(unnamed), "File")
        XCTAssertEqual(TransfersTableView.status(.progress), "In progress")
        XCTAssertEqual(TransfersTableView.status(.failed), "Failed")
    }

    func testPairingSheetHostsJoinFlow() throws {
        let engine = try RenderSupport.makeEngine(for: self)
        let model = PairingModel(engine: engine, onPaired: { _ in }, onSignInRequired: {})
        model.openJoin()
        try RenderSupport.hostOffscreen(PairingSheet(model: model, onClose: {}), "pairing sheet", size: CGSize(width: 420, height: 320))
    }

    /// Every SF Symbol the redesign uses must exist on the macOS 14 floor (spec icon map was unverified).
    func testSymbolsResolve() {
        let symbols = [
            "laptopcomputer.and.iphone", "clock.arrow.circlepath", "arrow.up.arrow.down.circle", "link.badge.plus", "gearshape",
            "circle.fill", "circle", "wifi.slash", "exclamationmark.triangle.fill",
            "laptopcomputer", "desktopcomputer", "terminal", "ellipsis.circle", "tray.and.arrow.down", "paperplane",
            "arrow.down", "arrow.up", "folder", "doc", "xmark", "trash", "pin", "pin.fill", "doc.on.doc",
            "envelope.badge", "arrow.triangle.2.circlepath", "questionmark.circle", "checkmark.circle.fill",
        ]
        for name in symbols {
            XCTAssertNotNil(NSImage(systemSymbolName: name, accessibilityDescription: nil), "missing SF Symbol \(name)")
        }
    }
}
