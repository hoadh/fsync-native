import AppKit
import XCTest
@testable import FsyncMacUI

/// The menubar window resizes from its bottom-left origin; the anchor must keep its top edge in place.
@MainActor
final class MenuWindowTopAnchorTests: XCTestCase {
    private func window(_ anchor: TopAnchorView = TopAnchorView()) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 100, y: 300, width: 280, height: 540),
                              styleMask: [.borderless], backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        window.contentView?.addSubview(anchor)
        return window
    }

    func testShrinkKeepsTopEdge() {
        let window = window()
        let top = window.frame.maxY
        window.setContentSize(NSSize(width: 280, height: 400))
        XCTAssertEqual(window.frame.height, 400)
        XCTAssertEqual(window.frame.maxY, top)
        XCTAssertEqual(window.frame.minX, 100, "only the vertical position is corrected")
    }

    func testGrowKeepsTopEdge() {
        let window = window()
        let top = window.frame.maxY
        window.setContentSize(NSSize(width: 280, height: 600))
        XCTAssertEqual(window.frame.maxY, top)
    }

    func testMoveIsLeftAlone() {
        let window = window()
        window.setFrameOrigin(NSPoint(x: 50, y: 50))
        XCTAssertEqual(window.frame.origin, NSPoint(x: 50, y: 50))
        window.setContentSize(NSSize(width: 280, height: 400))
        XCTAssertEqual(window.frame.maxY, 50 + 540, "the top stays where the move put it")
    }

    func testDetachedAnchorStopsCorrecting() {
        let anchor = TopAnchorView()
        let window = window(anchor)
        anchor.removeFromSuperview()
        window.setContentSize(NSSize(width: 280, height: 400))
        XCTAssertEqual(window.frame.origin.y, 300, "no observer is left behind on the old window")
    }
}
