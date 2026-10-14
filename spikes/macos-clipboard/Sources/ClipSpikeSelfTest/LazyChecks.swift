import AppKit
import ClipSpikeCore

enum LazyChecks {
    static let mib = 1 << 20

    static func run(_ r: Report, long: Bool) {
        listingDoesNotFetch(r)
        small(r)
        large(r, delay: 2, label: "200 MiB, fetch 2 s")
        if long { large(r, delay: 35, label: "200 MiB, fetch 35 s") }
        failingFetch(r)
        promises(r)
        keepsUIAlive(r)
        eager(r)
        ownerExit(r)
        manualItems(r, long: long)
    }

    /// Compares what a reader got with the bytes the fake remote makes.
    static func verify(_ r: Report, _ name: String, _ res: ChildResult, _ files: [FakeRemoteFile], extra: String = "") {
        let got = res.files
        let ok = !res.timedOut && got.count == files.count && zip(got, files).allSatisfy { g, f in
            (g["size"] as? Int) == f.size && (g["sha"] as? String) == Child.expectedSha(size: f.size)
        }
        r.check(name, ok, "files=\(got.count)/\(files.count) \(fmt(res.seconds))s\(res.timedOut ? " TIMEOUT" : "") \(extra)")
    }

    static func listingDoesNotFetch(_ r: Report) {
        let pb = uniquePasteboard()
        let writer = LazyFileWriter(files: [FakeRemoteFile(name: "a.bin", size: 10)], inbox: tempDir("inbox"))
        writer.write(to: pb)
        var change: PasteboardChange?
        let w = PasteboardWatcher(pasteboard: pb, frontmost: { nil }) { change = $0 }
        pb.clearContents(); writer.write(to: pb); w.poll()
        let types = runChild(["types", pb.name.rawValue], timeout: 10).json?["types"] as? [String] ?? []
        pump(until: 0.3) { true }
        r.check("lazy: listing types (watcher + other process) does not call the provider",
                writer.calls.isEmpty && types.contains("public.file-url"), "types=\(types.joined(separator: ","))")
        r.check("lazy: own lazy write is recognised by the watcher (echo guard)", change?.decision == .skip(.ownWrite))
    }

    static func small(_ r: Report) {
        let files = [FakeRemoteFile(name: "a.bin", size: 1024, delay: 0.3), FakeRemoteFile(name: "b.bin", size: 3 * mib, delay: 0.3),
                     FakeRemoteFile(name: "empty.bin", size: 0, delay: 0.3)]
        let pb = uniquePasteboard()
        let writer = LazyFileWriter(files: files, inbox: tempDir("inbox"))
        writer.write(to: pb)
        let res = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 60)
        verify(r, "lazy: 3 files (1 KiB, 3 MiB, 0 B), cross-process read, hashes match", res, files,
               extra: "provider calls=\(writer.calls.count)")
        r.check("lazy: provider ran once per file, only on read", writer.calls.count == 3 && writer.calls.allSatisfy(\.ok))
        r.check("lazy: reader waited for the fetch", res.seconds >= 0.3)
    }

    static func large(_ r: Report, delay: TimeInterval, label: String) {
        let files = [FakeRemoteFile(name: "big.bin", size: 200 * mib, delay: delay)]
        let pb = uniquePasteboard()
        let writer = LazyFileWriter(files: files, inbox: tempDir("inbox"))
        writer.write(to: pb)
        let res = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 300)
        verify(r, "lazy: \(label), reader blocks until bytes exist, hash matches", res, files)
    }

    static func failingFetch(_ r: Report) {
        let files = [FakeRemoteFile(name: "x.bin", size: 100, delay: 0.2, fails: true)]
        let pb = uniquePasteboard()
        let writer = LazyFileWriter(files: files, inbox: tempDir("inbox"))
        writer.write(to: pb)
        let res = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 30)
        r.check("lazy: failing fetch -> reader gets no file and does not hang",
                !res.timedOut && res.files.isEmpty && res.json != nil, "count=\(res.int("count") ?? -1) \(fmt(res.seconds))s")
    }

    /// A paste waits inside the provider on the owner's main thread; a timer on that thread must keep firing.
    static func keepsUIAlive(_ r: Report) {
        for keepAlive in [false, true] {
            let files = [FakeRemoteFile(name: "u.bin", size: mib, delay: 2)]
            let pb = uniquePasteboard()
            let writer = LazyFileWriter(files: files, inbox: tempDir("inbox"))
            writer.keepRunLoopAlive = keepAlive
            writer.write(to: pb)
            var ticks = 0
            let timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in ticks += 1 }
            let res = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 30)
            timer.invalidate()
            let onMain = writer.calls.first?.onMainThread ?? false
            r.info("lazy: provider thread is main=\(onMain); main-thread timer ticks during a 2 s fetch (keepRunLoopAlive=\(keepAlive))",
                   "ticks=\(ticks) read ok=\(res.files.count == 1)")
            if keepAlive { r.check("lazy: with run-loop spinning the main thread stays responsive (>= 10 ticks in 2 s)", ticks >= 10 && res.files.count == 1) }
        }
    }

    static func promises(_ r: Report) {
        let files = [FakeRemoteFile(name: "p1.bin", size: 2 * mib, delay: 0.3), FakeRemoteFile(name: "p2.bin", size: 10, delay: 0.3)]
        let pb = uniquePasteboard()
        let writer = FilePromiseWriter(files: files)
        writer.write(to: pb)
        var types: [String] = []
        pump(until: 0.1) { true }
        types = runChild(["types", pb.name.rawValue], timeout: 10).json?["types"] as? [String] ?? []
        r.info("promise: types on pasteboard", types.joined(separator: ", "))
        let plain = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 30)
        r.info("promise: reader that only understands file URLs (what a plain Cmd+V target sees)", "file urls=\(plain.int("count") ?? -1)")
        let res = runChild(["read-promises", pb.name.rawValue, tempDir("out").path, "8"], timeout: 60)
        let sorted = res.files.sorted { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
        let ok = !res.timedOut && sorted.count == 2 && zip(sorted, files).allSatisfy { g, f in
            (g["sha"] as? String) == Child.expectedSha(size: f.size) }
        // Finding (macOS 26.6): receivers are created (count > 0) but the provider is never asked, on named and
        // general pasteboards, with and without an app bundle / NSApplication.run(): promises need a drag session.
        r.info("promise: NSFilePromiseReceiver in another process (pasteboard, no drag)",
               "receivers=\(res.int("count") ?? -1) files delivered=\(sorted.count) served=\(writer.served) timedOut=\(res.timedOut) ok=\(ok)")
    }

    static func eager(_ r: Report) {
        let dir = tempDir("eager")
        let files = [FakeRemoteFile(name: "e1.bin", size: 5 * mib), FakeRemoteFile(name: "e2.bin", size: 7)]
        let urls = files.compactMap { try? $0.materialize(in: dir) }
        let pb = uniquePasteboard()
        EagerFileWriter.write(urls: urls, to: pb)
        verify(r, "eager: real file URLs, cross-process read, hashes match",
               runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 30), files)
    }

    /// The lazy entry belongs to its owner process: after it exits the data cannot be produced.
    static func ownerExit(_ r: Report) {
        let pb = uniquePasteboard()
        let stdin = Pipe()
        guard let (proc, out) = try? startChild(["own-lazy", pb.name.rawValue, "4096"], stdin: stdin) else {
            return r.check("owner exit: spawn", false)
        }
        pump(until: 10) { out.fileHandleForReading.availableData.isEmpty }
        let typesAlive = pb.types?.map(\.rawValue) ?? []
        try? stdin.fileHandleForWriting.close()
        pump(until: 10) { proc.isRunning }
        let res = runChild(["read-urls", pb.name.rawValue, tempDir("out").path], timeout: 20)
        r.check("owner exit: types listed while alive, no readable file after the owner quits (clear on exit)",
                typesAlive.contains("public.file-url") && res.files.isEmpty && !res.timedOut,
                "types after exit are stale=\((pb.types ?? []).contains(.fileURL)) urls=\(res.int("count") ?? -1)")
    }

    static func manualItems(_ r: Report, long: Bool) {
        r.manual("Finder Cmd+V of a lazy clip into a folder", "needs a person (Accessibility-driven keystrokes are blocked in this session); run `ClipSpikeSelfTest --offer` and paste")
        r.manual("Mail / Messages / Slack / VS Code / Chrome upload paste matrix", "needs a person with those apps")
        r.manual("1Password / Keychain / Safari autofill real-app copy captured as concealed", "needs a person with those apps")
        r.manual("Privacy alert ('Always allow') persistence across restarts on macOS 15.4+/26 and previous", "needs a person: alert shows only in a GUI session on programmatic read of another app's clip")
    }
}
