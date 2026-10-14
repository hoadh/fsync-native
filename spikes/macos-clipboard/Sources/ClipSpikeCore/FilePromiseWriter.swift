import AppKit

/// `NSFilePromiseProvider` on the pasteboard (the drag-and-drop mechanism, used for copy/paste):
/// receivers that understand promises (`NSFilePromiseReceiver`) ask for the file at a destination.
public final class FilePromiseWriter: NSObject, NSFilePromiseProviderDelegate {
    final class Provider: NSFilePromiseProvider {
        override func writableTypes(for pasteboard: NSPasteboard) -> [NSPasteboard.PasteboardType] {
            super.writableTypes(for: pasteboard) + [NSPasteboard.PasteboardType(SensitiveFilter.originType)]
        }
        override func pasteboardPropertyList(forType type: NSPasteboard.PasteboardType) -> Any? {
            type.rawValue == SensitiveFilter.originType ? Data() : super.pasteboardPropertyList(forType: type)
        }
    }

    private let files: [FakeRemoteFile]
    private let queue = OperationQueue()
    private let lock = NSLock()
    private var _served = 0
    public var served: Int { lock.lock(); defer { lock.unlock() }; return _served }

    public init(files: [FakeRemoteFile]) { self.files = files }

    @discardableResult
    public func write(to pasteboard: NSPasteboard) -> Bool {
        pasteboard.clearContents()
        let providers = files.indices.map { i -> Provider in
            let p = Provider(fileType: "public.data", delegate: self)
            p.userInfo = i
            return p
        }
        return pasteboard.writeObjects(providers)
    }

    public func filePromiseProvider(_ provider: NSFilePromiseProvider, fileNameForType fileType: String) -> String {
        files[(provider.userInfo as? Int) ?? 0].name
    }

    public func filePromiseProvider(_ provider: NSFilePromiseProvider, writePromiseTo url: URL,
                                    completionHandler: @escaping (Error?) -> Void) {
        let file = files[(provider.userInfo as? Int) ?? 0]
        do {
            let made = try file.materialize(in: url.deletingLastPathComponent().appendingPathComponent(".tmp-\(UUID().uuidString)"))
            try FileManager.default.moveItem(at: made, to: url)
            lock.lock(); _served += 1; lock.unlock()
            completionHandler(nil)
        } catch { completionHandler(error) }
    }

    public func operationQueue(for provider: NSFilePromiseProvider) -> OperationQueue { queue }
}
