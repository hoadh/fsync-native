import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

/// "Send to…" in both directions through the real bridge and engine, with `fsync-cli` as the other device.
@MainActor
final class FilesIntegrationTests: IntegrationTestCase {
    private func file(_ name: String, _ text: String) throws -> URL {
        let url = try tempDir().appendingPathComponent(name)
        try Data(text.utf8).write(to: url)
        return url
    }

    func testFileFromCliWaitsForAcceptThenLandsInDownloads() async throws {
        let downloads = try tempDir()
        let mac = try await pairedMac(cli: "f1", pasteboard: scratchPasteboard(), downloads: downloads)
        let files = try XCTUnwrap(mac.filesSync)
        let src = try file("from-cli.txt", "hello from the cli")
        let send = try spawn("f1", ["send", "mac-it", src.path, "--wait", "60"])
        let offered = await waitUntil(timeout: 60) { !files.offers.isEmpty }
        XCTAssertTrue(offered, send.diagnostics)
        XCTAssertEqual(files.offers.first?.files.first?.name, "from-cli.txt")
        XCTAssertFalse(FileManager.default.fileExists(atPath: downloads.appendingPathComponent("from-cli.txt").path))
        await files.accept(try XCTUnwrap(files.offers.first))
        XCTAssertNil(files.message)
        let landed = try String(contentsOf: downloads.appendingPathComponent("from-cli.txt"), encoding: .utf8)
        XCTAssertEqual(landed, "hello from the cli")
        try await send.expectExit(0, timeout: 30)
        XCTAssertEqual(mac.transfers?.rows.first?.state, .done)
    }

    func testAutoAcceptReceivesWithoutAsking() async throws {
        let downloads = try tempDir()
        let mac = try await pairedMac(cli: "f2", pasteboard: scratchPasteboard(), downloads: downloads)
        let settings = try XCTUnwrap(mac.clipSettings)
        await settings.update { $0.autoAcceptFiles = true }
        XCTAssertNil(settings.errorMessage)
        let send = try spawn("f2", ["send", "mac-it", try file("auto.txt", "auto").path, "--wait", "60"])
        let target = downloads.appendingPathComponent("auto.txt")
        let landed = await waitUntil(timeout: 60) { FileManager.default.fileExists(atPath: target.path) }
        XCTAssertTrue(landed, send.diagnostics)
        XCTAssertTrue(try XCTUnwrap(mac.filesSync).offers.isEmpty)
        try await send.expectExit(0, timeout: 30)
    }

    func testFileFromMacReachesCli() async throws {
        let mac = try await pairedMac(cli: "f3", pasteboard: scratchPasteboard())
        let out = try tempDir()
        let fetch = try spawn("f3", ["fetch", "--out", out.path, "--count", "1", "--timeout", "90"])
        _ = try await fetch.waitEvent("ready", timeout: 60)
        let devices = try XCTUnwrap(mac.devicesModel)
        await devices.reload()
        let cli = try XCTUnwrap(devices.devices.first { !$0.isSelf })
        let files = try XCTUnwrap(mac.filesSync)
        await files.send([try file("from-mac.txt", "hello from the mac")], to: cli.nodeId)
        XCTAssertNil(mac.filesSync?.message)
        let fetched = try await fetch.waitEvent("fetched", timeout: 60)
        let path = try XCTUnwrap(fetched["path"] as? String)
        XCTAssertEqual(try String(contentsOfFile: path, encoding: .utf8), "hello from the mac")
    }
}
