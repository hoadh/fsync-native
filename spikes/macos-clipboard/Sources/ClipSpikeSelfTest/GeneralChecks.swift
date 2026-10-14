import AppKit
import ClipSpikeCore

/// `--general`: the same questions on `NSPasteboard.general` (replaces the user's clipboard).
enum GeneralChecks {
    static func run(_ r: Report) {
        let pb = NSPasteboard.general
        var seen: PasteboardChange?
        let w = PasteboardWatcher(pasteboard: pb) { seen = $0 }
        pb.clearContents()
        pb.setString("spike-general-check", forType: .string)
        w.poll()
        r.check("general: change detected and classified", seen?.decision == .sync, "types=\(seen?.types ?? [])")
        let item = NSPasteboardItem()
        item.setString("not-a-real-secret", forType: .string)
        item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.concealedType))
        pb.clearContents(); pb.writeObjects([item]); w.poll()
        r.check("general: concealed item skipped", seen?.decision == .skip(.concealed))
        // Another process reads content the owner wrote: this is where an alert would block on a GUI session.
        let res = runChild(["types", "Apple CFPasteboard general"], timeout: 15)
        r.check("general: other process lists types of the general pasteboard", res.json != nil && !res.timedOut,
                "\(fmt(res.seconds))s")
        if #available(macOS 15.4, *) {
            r.info("general: accessBehavior", "\(pb.accessBehavior.rawValue)")
        }
        let files = [FakeRemoteFile(name: "g.bin", size: 4096, delay: 0.2)]
        let writer = LazyFileWriter(files: files, inbox: tempDir("inbox"))
        writer.write(to: pb)
        LazyChecks.verify(r, "general: lazy file read from another process", runChild(["read-urls", "Apple CFPasteboard general", tempDir("out").path], timeout: 30), files)
        pb.clearContents()
    }
}
