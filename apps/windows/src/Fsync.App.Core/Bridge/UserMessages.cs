using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Short, user-facing text for core errors (no internals, no content).</summary>
public static class UserMessages
{
    public static string For(Exception e) => e switch
    {
        FsyncException.Network n => $"Can't reach the fsync server ({n.msg}).",
        FsyncException.Auth => "Please sign in again.",
        FsyncException.ReauthRequired => "For this, sign in again first.",
        FsyncException.PeerOffline => "That device is offline.",
        FsyncException.PeerRefused => "That device removed this PC. Pair again to send to it.",
        FsyncException.NotPaired => "That device is not paired.",
        FsyncException.Cancelled => "Cancelled.",
        FsyncException.Integrity => "The data failed verification and was discarded.",
        FsyncException.TooLarge => "That is larger than the size limit.",
        FsyncException.NotFound => "It no longer exists.",
        FsyncException.Invalid i => i.msg,
        FsyncException.Io io => $"File error: {io.msg}",
        FsyncException.Internal x => $"Something went wrong ({x.msg}).",
        FsyncException.EngineAlreadyRunning => EngineBridge.AlreadyRunningMessage,
        FsyncException.HistoryKeyMismatch => "The clipboard history was encrypted with a different key.",
        // bindgen-cs sets Message to "@msg=..." or the .NET default: never show it as is.
        FsyncException f => $"Something went wrong ({f.GetType().Name}).",
        _ => e.Message,
    };
}
