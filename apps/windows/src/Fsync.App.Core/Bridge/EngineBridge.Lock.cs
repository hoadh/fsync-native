using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Core's exclusive data-dir lock (engine.lock): only one engine per data folder.</summary>
public sealed partial class EngineBridge
{
    public const string AlreadyRunningMessage =
        "fsync is already running for this data folder. Close the other fsync, then start it again.";

    /// <summary>Start failed because another engine holds engine.lock (no retry).</summary>
    public bool AlreadyRunning { get; private set; }

    /// <summary>core: <c>Invalid("another fsync engine is already running on this data directory")</c>.</summary>
    internal static bool IsDataDirLocked(FsyncException.Invalid e) =>
        e.msg.Contains("already running", StringComparison.OrdinalIgnoreCase);
}
