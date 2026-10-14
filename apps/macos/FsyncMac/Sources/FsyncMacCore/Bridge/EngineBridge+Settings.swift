extension EngineBridge {
    /// First 8 characters of this Mac's node id for the About line; nil before the engine exists.
    public func nodeIdPrefix() async -> String? {
        guard let engine else { return nil }
        return await Task.detached { String(engine.nodeId().prefix(8)) }.value
    }
}
