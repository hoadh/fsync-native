import AppKit
import XCTest

extension XCTestCase {
    /// A private named pasteboard; released after the test.
    @MainActor
    func scratchPasteboard() -> NSPasteboard {
        let board = NSPasteboard(name: NSPasteboard.Name("app.fsync.mac.tests.\(UUID().uuidString)"))
        addTeardownBlock { board.releaseGlobally() }
        return board
    }
}

/// A small real PNG / TIFF produced by AppKit.
enum TestImages {
    static func bitmap() -> NSBitmapImageRep {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 8, pixelsHigh: 8, bitsPerSample: 8, samplesPerPixel: 4,
                                   hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        for x in 0..<8 { for y in 0..<8 { rep.setColor(NSColor(red: CGFloat(x) / 8, green: CGFloat(y) / 8, blue: 0.5, alpha: 1), atX: x, y: y) } }
        return rep
    }
    static var png: Data { bitmap().representation(using: .png, properties: [:])! }
    static var tiff: Data { bitmap().tiffRepresentation! }
}
