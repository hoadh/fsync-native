import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class ClipSettingsModelTests: EngineTestCase {
    private func model(onChange: @escaping @MainActor (Settings) -> Void = { _ in }) throws -> (ClipSettingsModel, FsyncEngine) {
        let engine = try makeEngine(try tempDir(), secrets: AppSecrets(store: makeStore()))
        return (ClipSettingsModel(engine: engine, onChange: onChange), engine)
    }

    func testLoadsEngineDefaults() throws {
        let (m, engine) = try model()
        XCTAssertEqual(m.settings, try engine.settings())
        XCTAssertEqual(m.settings.clipMode, .auto)
    }

    func testUpdatePersistsInEngineAndNotifies() async throws {
        var notified: [Settings] = []
        let (m, engine) = try model { notified.append($0) }
        await m.update { $0.clipMode = .manual; $0.retentionDays = 7; $0.maxClipBytes = 10 << 20 }
        let stored = try engine.settings()
        XCTAssertEqual(stored.clipMode, .manual)
        XCTAssertEqual(stored.retentionDays, 7)
        XCTAssertEqual(stored.maxClipBytes, 10 << 20)
        XCTAssertEqual(notified, [stored])
        XCTAssertNil(m.errorMessage)
    }

    func testValuesAreClampedToCoreLimits() async throws {
        let (m, engine) = try model()
        await m.update { $0.retentionDays = 0; $0.retentionBytes = 0; $0.maxClipBytes = 5 }
        var s = try engine.settings()
        XCTAssertEqual(s.retentionDays, 1)
        XCTAssertEqual([s.retentionBytes, s.maxClipBytes], [ClipSettingsModel.minBytes, ClipSettingsModel.minBytes])
        await m.update { $0.retentionDays = 99_999 }
        s = try engine.settings()
        XCTAssertEqual(s.retentionDays, 3650)
        XCTAssertNil(m.errorMessage)
    }

    func testUnchangedSettingsAreNotSavedOrAnnounced() async throws {
        var count = 0
        let (m, _) = try model { _ in count += 1 }
        await m.update { _ in }
        XCTAssertEqual(count, 0)
    }

    func testDenylistAddRemoveCleansInput() async throws {
        let (m, engine) = try model()
        await m.addToDenylist("  org.example.vault ")
        await m.addToDenylist("org.example.vault")
        await m.addToDenylist("   ")
        XCTAssertEqual(try engine.settings().denylist, ["org.example.vault"])
        await m.removeFromDenylist("org.example.vault")
        XCTAssertEqual(try engine.settings().denylist, [])
    }
}
