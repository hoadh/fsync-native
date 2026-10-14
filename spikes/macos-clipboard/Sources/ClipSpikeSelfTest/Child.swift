import AppKit
import ClipSpikeCore
import CryptoKit
import Foundation

/// Helper modes run in a separate process, so reads cross a real process boundary like a paste does.
/// `--child MODE PASTEBOARD [ARGS]`; results are one JSON line on stdout.
enum Child {
    static func sha256(of url: URL) throws -> (size: Int, hex: String) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        var size = 0
        while let data = try handle.read(upToCount: 1 << 20), !data.isEmpty { hasher.update(data: data); size += data.count }
        return (size, hasher.finalize().map { String(format: "%02x", $0) }.joined())
    }

    static func expectedSha(size: Int) -> String {
        var hasher = SHA256()
        var off = 0
        while off < size { let n = min(1 << 20, size - off); hasher.update(data: FakeRemoteFile.chunk(offset: off, count: n)); off += n }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    static func emit(_ object: [String: Any]) {
        let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data()
        print(String(decoding: data, as: UTF8.self))
        fflush(stdout)
    }

    static func run(_ args: [String]) -> Never {
        guard args.count >= 2 else { exit(2) }
        let pb = NSPasteboard(name: NSPasteboard.Name(args[1]))
        let started = Date()
        switch args[0] {
        case "types": emit(["types": (pb.types ?? []).map(\.rawValue)])
        case "read-urls": readURLs(pb, outDir: URL(fileURLWithPath: args[2]), started: started)
        case "read-promises": readPromises(pb, outDir: URL(fileURLWithPath: args[2]), started: started)
        case "write-text":
            pb.clearContents()
            pb.setString(args[2], forType: .string)
            emit(["changeCount": pb.changeCount])
        case "own-lazy": ownLazy(pb, size: Int(args[2]) ?? 1024)
        default: exit(2)
        }
        exit(0)
    }

    private static func readURLs(_ pb: NSPasteboard, outDir: URL, started: Date) {
        let urls = (pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]) ?? []
        var files: [[String: Any]] = []
        for (i, url) in urls.enumerated() {
            // Finder's paste copies the file the URL points at.
            let dest = outDir.appendingPathComponent("\(i)-\(url.lastPathComponent)")
            do {
                try FileManager.default.copyItem(at: url, to: dest)
                let s = try sha256(of: dest)
                files.append(["name": url.lastPathComponent, "size": s.size, "sha": s.hex])
            } catch { files.append(["name": url.lastPathComponent, "error": "\(error)"]) }
        }
        emit(["count": urls.count, "files": files, "seconds": Date().timeIntervalSince(started)])
    }

    private static func readPromises(_ pb: NSPasteboard, outDir: URL, started: Date) {
        NSApplication.shared.setActivationPolicy(.accessory)
        NSApplication.shared.finishLaunching()
        let receivers = (pb.readObjects(forClasses: [NSFilePromiseReceiver.self], options: nil) as? [NSFilePromiseReceiver]) ?? []
        FileHandle.standardError.write(Data("[child] receivers=\(receivers.count) types=\(receivers.first?.fileTypes ?? []) names=\(receivers.first?.fileNames ?? [])\n".utf8))
        var files: [[String: Any]] = []
        var errors: [String] = []
        let lock = NSLock()
        let queue = OperationQueue()
        let group = DispatchGroup()
        for receiver in receivers {
            group.enter()
            receiver.receivePromisedFiles(atDestination: outDir, options: [:], operationQueue: queue) { url, error in
                FileHandle.standardError.write(Data("callback \(url.lastPathComponent) err=\(String(describing: error))\n".utf8))
                lock.lock(); defer { lock.unlock(); group.leave() }
                if let error { errors.append("\(error)"); return }
                if let s = try? sha256(of: url) { files.append(["name": url.lastPathComponent, "size": s.size, "sha": s.hex]) }
            }
        }
        var done = false
        group.notify(queue: .main) { done = true }
        let deadline = Date().addingTimeInterval(Double(CommandLine.arguments.last ?? "") ?? 120)
        while !done && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
        emit(["count": receivers.count, "files": files, "errors": errors, "timedOut": !done,
              "seconds": Date().timeIntervalSince(started)])
    }

    /// Owns a lazy clip until stdin closes; used to see what remains after the owner exits.
    private static func ownLazy(_ pb: NSPasteboard, size: Int) {
        let inbox = FileManager.default.temporaryDirectory.appendingPathComponent("spike-own-\(UUID().uuidString)")
        let writer = LazyFileWriter(files: [FakeRemoteFile(name: "owned.bin", size: size)], inbox: inbox)
        writer.write(to: pb)
        emit(["ready": true])
        DispatchQueue.global().async { _ = FileHandle.standardInput.readDataToEndOfFile(); exit(0) }
        withExtendedLifetime(writer) { RunLoop.main.run() }
    }
}
