import AppKit
import ServiceManagement
import XCTest
@testable import FsyncMacCore

@MainActor
final class SettingsTests: XCTestCase {
    func testStatusMapping() {
        XCTAssertEqual(LaunchAtLogin.map(.enabled), .enabled)
        XCTAssertEqual(LaunchAtLogin.map(.notRegistered), .disabled)
        XCTAssertEqual(LaunchAtLogin.map(.requiresApproval), .requiresApproval)
        XCTAssertEqual(LaunchAtLogin.map(.notFound), .unavailable)
    }

    /// The test runner is not an app bundle, so the system reports no registrable app; reading must not crash.
    func testStatusReadableOutsideBundle() {
        let launch = LaunchAtLogin()
        XCTAssertEqual(launch.status == .unavailable, !launch.canToggle)
        launch.refresh()
    }

    func testPairingCodeIsMarkedConcealedAndTransient() throws {
        let board = NSPasteboard(name: NSPasteboard.Name("app.fsync.mac.tests.\(UUID().uuidString)"))
        addTeardownBlock { board.releaseGlobally() }
        XCTAssertTrue(PairingPasteboard.copy("ABCD1234", to: board))
        XCTAssertEqual(board.string(forType: .string), "ABCD1234")
        let types = try XCTUnwrap(board.types)
        XCTAssertTrue(types.contains(PairingPasteboard.concealedType))
        XCTAssertTrue(types.contains(PairingPasteboard.transientType))
        XCTAssertEqual(PairingPasteboard.concealedType.rawValue, "org.nspasteboard.ConcealedType")
        XCTAssertEqual(PairingPasteboard.transientType.rawValue, "org.nspasteboard.TransientType")
    }

    func testCopyReplacesPreviousContents() {
        let board = NSPasteboard(name: NSPasteboard.Name("app.fsync.mac.tests.\(UUID().uuidString)"))
        addTeardownBlock { board.releaseGlobally() }
        board.clearContents()
        board.setString("old", forType: .string)
        PairingPasteboard.copy("NEW", to: board)
        XCTAssertEqual(board.string(forType: .string), "NEW")
    }
}

final class PrivateDirTests: XCTestCase {
    func testExistingLooseDirIsTightenedTo0700() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("fsync-dir-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o755])
        try AppConfig.createPrivateDir(dir)
        let mode = try FileManager.default.attributesOfItem(atPath: dir.path)[.posixPermissions] as? Int
        XCTAssertEqual(mode, 0o700)
    }

    func testFallbackDirIsPrivate() throws {
        let dir = AppConfig.fallbackDataDir()
        let mode = try FileManager.default.attributesOfItem(atPath: dir.path)[.posixPermissions] as? Int
        XCTAssertEqual(mode, 0o700)
    }
}
