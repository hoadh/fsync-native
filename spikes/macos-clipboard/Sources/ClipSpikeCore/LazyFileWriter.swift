import AppKit

/// Lazy file clip: each pasteboard item advertises `public.file-url` but the bytes (and the URL)
/// only come into existence when a reader asks for that type. Readers that only list types
/// (clipboard history tools, our own watcher) never trigger the fetch.
public final class LazyFileWriter: NSObject, NSPasteboardItemDataProvider {
    public struct Call { public let index: Int; public let started: Date; public let seconds: TimeInterval; public let ok: Bool; public let onMainThread: Bool }

    private let files: [FakeRemoteFile]
    private let inbox: URL
    /// Phase 12 default: true. The plain blocking variant is kept to measure the difference.
    public var keepRunLoopAlive = true
    private let lock = NSLock()
    private var _calls: [Call] = []
    public var calls: [Call] { lock.lock(); defer { lock.unlock() }; return _calls }

    public init(files: [FakeRemoteFile], inbox: URL) { self.files = files; self.inbox = inbox }

    /// Replaces the pasteboard contents. The writer must stay alive while the clip is current.
    @discardableResult
    public func write(to pasteboard: NSPasteboard) -> Bool {
        pasteboard.clearContents()
        let items = files.indices.map { i -> NSPasteboardItem in
            let item = NSPasteboardItem()
            item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.originType))
            item.setDataProvider(self, forTypes: [.fileURL])
            return item
        }
        return pasteboard.writeObjects(items)
    }

    public func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        guard type == .fileURL, let pasteboard, case let index = pasteboard.index(of: item),
              index != NSNotFound, index < files.count else { return }
        let started = Date()
        let onMain = Thread.isMainThread
        var ok = false
        defer {
            lock.lock(); _calls.append(Call(index: index, started: started, seconds: Date().timeIntervalSince(started), ok: ok, onMainThread: onMain)); lock.unlock()
        }
        // Blocking here is the point: the reader waits for the bytes (phase 12 calls `fetch_file_to`).
        guard let url = fetch(index) else { return }
        item.setString(url.absoluteString, forType: .fileURL)
        ok = true
    }

    /// Runs the fetch off the calling thread and, when that is the main thread, keeps the main run loop
    /// turning so the app's UI (menu, progress, timers) stays alive while a paste waits for bytes.
    private func fetch(_ index: Int) -> URL? {
        guard keepRunLoopAlive, Thread.isMainThread else {
            return try? files[index].materialize(in: inbox.appendingPathComponent(String(index)))
        }
        var result: URL?
        let done = DispatchSemaphore(value: 0)
        let finished = Flag()
        DispatchQueue.global().async {
            result = try? self.files[index].materialize(in: self.inbox.appendingPathComponent(String(index)))
            finished.set()
            done.signal()
        }
        while !finished.value { RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        done.wait()
        return result
    }

    public func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {}
}

/// Eager fallback: the files already exist, so plain file URLs go on the pasteboard.
public enum EagerFileWriter {
    @discardableResult
    public static func write(urls: [URL], to pasteboard: NSPasteboard) -> Bool {
        pasteboard.clearContents()
        let items = urls.map { url -> NSPasteboardItem in
            let item = NSPasteboardItem()
            item.setString(url.absoluteString, forType: .fileURL)
            item.setData(Data(), forType: NSPasteboard.PasteboardType(SensitiveFilter.originType))
            return item
        }
        return pasteboard.writeObjects(items)
    }
}

final class Flag: @unchecked Sendable {
    private let lock = NSLock()
    private var flag = false
    var value: Bool { lock.lock(); defer { lock.unlock() }; return flag }
    func set() { lock.lock(); flag = true; lock.unlock() }
}
