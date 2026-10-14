import AppKit
import ClipSpikeCore

/// `--offer`: puts lazy files on the GENERAL pasteboard (replaces the user's clipboard) and logs each
/// provider call, so a person can paste into Finder/Mail/etc. and see who asked, when, and for what.
enum Offer {
    /// `--offer-promise [--general]`: NSFilePromiseProvider clip, served from a real NSApplication run loop.
    static func promise(pasteboard: NSPasteboard) -> Never {
        let files = [FakeRemoteFile(name: "fsync-promise.txt", size: 1024, delay: 0.5)]
        let writer = FilePromiseWriter(files: files)
        writer.write(to: pasteboard)
        print("Offered promise clip on \(pasteboard.name.rawValue)")
        withExtendedLifetime(writer) { NSApplication.shared.run() }
        exit(0)
    }

    static func run(size mib: Int, delay: Double) -> Never {
        let files = [FakeRemoteFile(name: "fsync-lazy-small.txt", size: 1024, delay: 0.5),
                     FakeRemoteFile(name: "fsync-lazy-big.bin", size: mib * (1 << 20), delay: delay)]
        let writer = LazyFileWriter(files: files, inbox: tempDir("offer-inbox"))
        writer.write(to: .general)
        print("Offered \(files.count) lazy files on the general pasteboard. Paste now (Cmd+V in Finder). Ctrl-C to quit.")
        var shown = 0
        let timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { _ in
            for c in writer.calls.dropFirst(shown) { print("provider call: file \(c.index) ok=\(c.ok) took \(fmt(c.seconds))s") }
            shown = writer.calls.count
        }
        withExtendedLifetime((writer, timer)) { RunLoop.main.run() }
        exit(0)
    }
}
