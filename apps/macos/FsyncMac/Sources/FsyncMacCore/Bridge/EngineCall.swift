import FsyncCoreKit

/// Engine calls block, so they never run on the main actor.
public enum EngineCall {
    public static func run<T: Sendable>(
        _ engine: FsyncEngineProtocol,
        _ body: @escaping @Sendable (FsyncEngineProtocol) throws -> T
    ) async -> Result<T, Error> {
        await Task.detached { Result { try body(engine) } }.value
    }
}
