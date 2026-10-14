import Foundation

private final class Slot<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: T?
    private var done = false
    func finish(_ v: T?) { lock.lock(); value = v; done = true; lock.unlock() }
    var result: (done: Bool, value: T?) { lock.lock(); defer { lock.unlock() }; return (done, value) }
}

/// Runs a blocking call (engine fetch) for a pasteboard data provider. AppKit calls providers on the
/// main thread while the reader waits; the work runs on a background queue and the main run loop keeps
/// turning meanwhile, so the menubar UI stays alive during a long paste (ADR 009 decision 3).
enum BlockingFetch {
    static func run<T: Sendable>(_ work: @escaping @Sendable () -> T?) -> T? {
        guard Thread.isMainThread else { return work() }
        let slot = Slot<T>()
        DispatchQueue.global(qos: .userInitiated).async { slot.finish(work()) }
        while !slot.result.done { RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        return slot.result.value
    }
}
