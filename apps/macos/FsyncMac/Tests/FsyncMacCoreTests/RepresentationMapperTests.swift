import AppKit
import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class RepresentationMapperTests: EngineTestCase {
    private func clip(_ read: RepresentationMapper.ClipRead, file: StaticString = #filePath, line: UInt = #line) throws -> LocalClip {
        guard case .clip(let c) = read else {
            XCTFail("not a clip: \(read)", file: file, line: line)
            throw CocoaError(.fileReadUnknown)
        }
        return c
    }

    func testPlainText() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setString("héllo", forType: .string)
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.reps.map(\.mime), ["text/plain"])
        XCTAssertEqual(String(data: c.reps[0].data, encoding: .utf8), "héllo")
        XCTAssertEqual(c.preview, "héllo")
        XCTAssertTrue(c.files.isEmpty)
        XCTAssertFalse(c.sensitive)
    }

    func testRichTextKeepsPlainHtmlAndRtf() throws {
        let pb = scratchPasteboard()
        pb.clearContents()
        let item = NSPasteboardItem()
        item.setString("bold", forType: .string)
        item.setData(Data("<b>bold</b>".utf8), forType: .html)
        item.setData(Data("{\\rtf1 bold}".utf8), forType: .rtf)
        pb.writeObjects([item])
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.reps.map(\.mime), ["text/plain", "text/html", "text/rtf"])
    }

    func testPreviewIsCut() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setString(String(repeating: "a", count: 500), forType: .string)
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.preview?.count, 200)
        XCTAssertEqual(c.reps[0].data.count, 500, "the full text is sent")
    }

    func testPngPassesThroughUnchanged() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setData(TestImages.png, forType: .png)
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.reps.map(\.mime), ["image/png"])
        XCTAssertEqual(c.reps[0].data, TestImages.png)
    }

    func testTiffIsConvertedToPng() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setData(TestImages.tiff, forType: .tiff)
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.reps.map(\.mime), ["image/png"])
        XCTAssertEqual(Array(c.reps[0].data.prefix(4)), [0x89, 0x50, 0x4E, 0x47], "PNG signature")
        XCTAssertEqual(NSBitmapImageRep(data: c.reps[0].data)?.pixelsWide, 8)
    }

    func testTextWinsOverImage() throws {
        let pb = scratchPasteboard()
        pb.clearContents()
        let item = NSPasteboardItem()
        item.setString("cell", forType: .string)
        item.setData(TestImages.tiff, forType: .tiff)
        pb.writeObjects([item])
        XCTAssertEqual(try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20)).reps.map(\.mime), ["text/plain"])
    }

    func testFileURLsBecomeFilesAndSkipFolders() throws {
        let dir = try tempDir()
        let file = dir.appendingPathComponent("a.txt")
        try Data("x".utf8).write(to: file)
        let folder = dir.appendingPathComponent("sub")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pb = scratchPasteboard()
        pb.clearContents(); pb.writeObjects([file as NSURL, folder as NSURL])
        let c = try clip(RepresentationMapper.read(pb, maxBytes: 1 << 20))
        XCTAssertEqual(c.files.map { URL(fileURLWithPath: $0).resolvingSymlinksInPath().path }, [file.resolvingSymlinksInPath().path])
        XCTAssertTrue(c.reps.isEmpty)
        XCTAssertEqual(c.preview, "a.txt")
    }

    func testOnlyFoldersIsEmpty() throws {
        let dir = try tempDir()
        let pb = scratchPasteboard()
        pb.clearContents(); pb.writeObjects([dir as NSURL])
        XCTAssertEqual(RepresentationMapper.read(pb, maxBytes: 1 << 20), .empty)
    }

    func testEmptyAndTooLarge() {
        let pb = scratchPasteboard()
        pb.clearContents()
        XCTAssertEqual(RepresentationMapper.read(pb, maxBytes: 1 << 20), .empty)
        pb.setString("", forType: .string)
        XCTAssertEqual(RepresentationMapper.read(pb, maxBytes: 1 << 20), .empty)
        pb.clearContents(); pb.setString(String(repeating: "z", count: 100), forType: .string)
        XCTAssertEqual(RepresentationMapper.read(pb, maxBytes: 99), .tooLarge(bytes: 100))
        XCTAssertNotEqual(RepresentationMapper.read(pb, maxBytes: 100), .tooLarge(bytes: 100))
    }

    func testMimeMapping() {
        XCTAssertEqual(RepresentationMapper.pasteboardType(forMime: "text/plain"), .string)
        XCTAssertEqual(RepresentationMapper.pasteboardType(forMime: "text/html"), .html)
        XCTAssertEqual(RepresentationMapper.pasteboardType(forMime: "text/rtf"), .rtf)
        XCTAssertEqual(RepresentationMapper.pasteboardType(forMime: "image/png"), .png)
        XCTAssertNil(RepresentationMapper.pasteboardType(forMime: "application/x-unknown"))
    }

    func testInvalidUtf8TextIsNotPasted() {
        XCTAssertNil(RepresentationMapper.payload(Data([0xFF, 0xFE, 0x41]), for: .string))
        XCTAssertEqual(RepresentationMapper.payload(Data([0xFF]), for: .png), Data([0xFF]))
    }
}

@MainActor
final class RepresentationSnapshotTests: EngineTestCase {
    private func tiff() throws -> Data {
        let rep = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 64, bitsPerSample: 8,
                                                 samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                                 colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        return try XCTUnwrap(rep.representation(using: .tiff, properties: [:]))
    }

    func testTiffIsConvertedToPngInBuildNotInSnapshot() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setData(try tiff(), forType: .tiff)
        let snap = RepresentationMapper.snapshot(pb)
        XCTAssertNotNil(snap.tiff)
        XCTAssertNil(snap.png)
        guard case .clip(let c) = snap.build(maxBytes: 1 << 24) else { return XCTFail("no clip") }
        XCTAssertEqual(c.reps.map(\.mime), ["image/png"])
        XCTAssertEqual(c.reps[0].data.prefix(4), Data([0x89, 0x50, 0x4E, 0x47]))
    }

    func testHugeTiffIsRefusedWithoutDecoding() throws {
        let pb = scratchPasteboard()
        pb.clearContents(); pb.setData(try tiff(), forType: .tiff)
        let snap = RepresentationMapper.snapshot(pb)
        guard case .tooLarge = snap.build(maxBytes: 1) else { return XCTFail("expected tooLarge") }
    }

    func testTextWinsOverImage() throws {
        let pb = scratchPasteboard()
        pb.clearContents()
        let item = NSPasteboardItem()
        item.setString("cells", forType: .string)
        item.setData(try tiff(), forType: .tiff)
        pb.writeObjects([item])
        guard case .clip(let c) = RepresentationMapper.snapshot(pb).build(maxBytes: 1 << 24) else { return XCTFail("no clip") }
        XCTAssertEqual(c.reps.map(\.mime), ["text/plain"])
    }
}
