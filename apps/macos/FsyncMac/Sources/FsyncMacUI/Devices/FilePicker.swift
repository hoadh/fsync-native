import AppKit
import UniformTypeIdentifiers

/// Choosing files to send: open panel and drag-and-drop payloads.
enum FilePicker {
    @MainActor
    static func chooseFiles() -> [URL]? {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = true
        panel.prompt = "Send"
        return panel.runModal() == .OK && !panel.urls.isEmpty ? panel.urls : nil
    }

    /// Resolves dropped providers to file and folder URLs and reports them on the main queue.
    static func fileURLs(from providers: [NSItemProvider], completion: @escaping ([URL]) -> Void) {
        nonisolated(unsafe) let completion = completion
        let group = DispatchGroup()
        let lock = NSLock()
        nonisolated(unsafe) var urls: [URL] = []
        for provider in providers where provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            group.enter()
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                let kind = try? url?.resourceValues(forKeys: [.isRegularFileKey, .isDirectoryKey])
                if let url, url.isFileURL, kind?.isRegularFile == true || kind?.isDirectory == true {
                    lock.lock(); urls.append(url); lock.unlock()
                }
                group.leave()
            }
        }
        group.notify(queue: .main) {
            let found = urls
            if !found.isEmpty { completion(found) }
        }
    }
}
