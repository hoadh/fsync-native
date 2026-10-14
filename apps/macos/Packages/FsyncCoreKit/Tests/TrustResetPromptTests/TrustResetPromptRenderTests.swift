import AppKit
import FsyncCoreKit
import SwiftUI
import XCTest
@testable import TrustResetPrompt

/// Renders each state with `ImageRenderer`. Set `TRUST_RESET_SNAPSHOT_DIR` to keep the PNGs.
/// Besides "produces a real image", the tests compare sizes and brightness between states;
/// they are still smoke tests for layout, not pixel-exact snapshots (controls are not driven).
@MainActor
final class TrustResetPromptRenderTests: XCTestCase {
    private let info = TrustResetInfo(
        resetId: "ab12cd34ef567890" + String(repeating: "0", count: 48), devices: 3,
        deviceNames: ["Alex's MacBook Pro", "Alex's iPhone", String(repeating: "Very long device name ", count: 6)])

    private func content(
        _ info: TrustResetInfo?, _ outcome: TrustResetOutcome = .unanswered, busy: Bool = false, error: String? = nil, notice: Bool = false
    ) -> some View {
        TrustResetPromptContent(
            info: info, outcome: outcome, isBusy: busy, errorMessage: error, errorIsNotice: notice, onKeep: { _ in }, onTrust: { _ in }, onDone: {})
    }

    private struct Rendered { let size: CGSize; let luma: Double }

    @discardableResult
    private func render<V: View>(_ view: V, name: String, scheme: ColorScheme = .light, snapshot: Bool = true) throws -> Rendered {
        // Resolve NSColor-based colors (window background) under the requested appearance.
        let appearance = try XCTUnwrap(NSAppearance(named: scheme == .dark ? .darkAqua : .aqua))
        var rendered: NSImage?
        appearance.performAsCurrentDrawingAppearance {
            let renderer = ImageRenderer(content: view.environment(\.colorScheme, scheme))
            renderer.scale = 2
            rendered = renderer.nsImage
        }
        let image = try XCTUnwrap(rendered, "\(name): no image")
        XCTAssertGreaterThan(image.size.width, 100, name)
        XCTAssertGreaterThan(image.size.height, 50, name)
        let rep = try XCTUnwrap(image.tiffRepresentation.flatMap(NSBitmapImageRep.init(data:)), name)
        let png = try XCTUnwrap(rep.representation(using: .png, properties: [:]), name)
        XCTAssertGreaterThan(png.count, 1000, name)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: png), name)
        var sum = 0.0, n = 0.0
        for x in stride(from: 0, to: bitmap.pixelsWide, by: 8) {
            for y in stride(from: 0, to: bitmap.pixelsHigh, by: 8) {
                if let c = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) {
                    sum += 0.3 * c.redComponent + 0.59 * c.greenComponent + 0.11 * c.blueComponent
                    n += 1
                }
            }
        }
        if snapshot { writeSnapshot(png, name) }
        return Rendered(size: image.size, luma: sum / max(n, 1))
    }

    private func writeSnapshot(_ png: Data, _ name: String) {
        do {
            if let dir = ProcessInfo.processInfo.environment["TRUST_RESET_SNAPSHOT_DIR"], !dir.isEmpty {
                try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
                try png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
            }
        } catch { XCTFail("snapshot \(name): \(error)") }
    }

    func testPendingStates() throws {
        let light = try render(content(info), name: "pending-light")
        let dark = try render(content(info), name: "pending-dark", scheme: .dark)
        XCTAssertLessThan(dark.luma, light.luma - 0.3, "dark appearance must be dark")
        XCTAssertEqual(dark.size, light.size)
        try render(content(info, busy: true), name: "pending-busy")
        let notice = try render(content(info, error: TrustResetMapping.listChanged.text, notice: true), name: "pending-notice")
        let error = try render(content(info, error: "Couldn't reach the server. Check your connection and try again."), name: "pending-error")
        XCTAssertGreaterThan(notice.size.height, light.size.height, "message adds a line")
        XCTAssertGreaterThan(error.size.height, light.size.height)
    }

    func testManyDevicesScrollWithBoundedHeight() throws {
        let many = TrustResetInfo(
            resetId: String(repeating: "ef", count: 32), devices: 20,
            deviceNames: (1...20).map { "Device number \($0)" })
        // ImageRenderer draws no ScrollView content, so this one is measured but not saved as a PNG.
        let big = try render(content(many), name: "pending-many-devices", snapshot: false)
        let small = try render(content(info), name: "pending-light", snapshot: false)
        XCTAssertLessThan(big.size.height, small.size.height + 160, "list is capped, not 20 rows tall")
    }

    func testOutcomeStates() throws {
        let trusted = try render(content(nil, .trusted), name: "outcome-trusted")
        try render(content(nil, .removedNeedsPairing), name: "outcome-needs-pairing")
        try render(content(nil, .declined), name: "outcome-declined")
        try render(content(nil, .trusted), name: "outcome-trusted-dark", scheme: .dark)
        let pending = try render(content(info), name: "pending-light", snapshot: false)
        XCTAssertLessThan(trusted.size.height, pending.size.height)
    }

    func testEmptyState() throws {
        try render(content(nil), name: "empty")
    }
}
