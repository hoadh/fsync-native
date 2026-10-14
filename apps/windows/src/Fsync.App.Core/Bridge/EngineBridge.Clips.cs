using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Clipboard, file and transfer callbacks (phase 14).</summary>
public sealed partial class EngineBridge
{
    /// <summary>A peer published a clip (UI thread).</summary>
    public event Action<ClipItem>? RemoteClip;
    /// <summary>A peer pushed files to this PC ("Send to"), waiting to be accepted (UI thread).</summary>
    public event Action<ClipItem>? IncomingFiles;
    /// <summary>Progress / completion of a pull or push (UI thread).</summary>
    public event Action<TransferEvent>? Transfer;

    /// <summary>True once the engine exists (calls may still fail while offline).</summary>
    public bool HasEngine => _engine is not null;

    /// <summary>
    /// Blocking engine call on the calling thread: for threads that must wait anyway (the
    /// clipboard STA rendering a delayed format, Explorer's copy thread). Never the UI thread.
    /// </summary>
    public T Call<T>(Func<IFsyncEngine, T> call) => call(Engine);

    internal void RaiseRemoteClip(ClipItem item) => RemoteClip?.Invoke(item);
    internal void RaiseIncomingFiles(ClipItem offer) => IncomingFiles?.Invoke(offer);
    internal void RaiseTransfer(TransferEvent ev) => Transfer?.Invoke(ev);
}
