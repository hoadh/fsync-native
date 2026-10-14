namespace Fsync.App.Clipboard;

/// <summary>
/// Cancellation of one clipboard write. The next write cancels it; whoever comes last (the write
/// finishing, or the next write replacing it) disposes the source, so none leak and the current
/// one stays usable.
/// </summary>
internal sealed class WriteTicket
{
    private readonly CancellationTokenSource _cts = new();
    private int _done, _replaced;

    public CancellationToken Token => _cts.Token;

    public void Cancel()
    {
        try { _cts.Cancel(); }
        catch (ObjectDisposedException) { }
    }

    /// <summary>A newer write took over: cancel, and dispose if this write already finished.</summary>
    public void Replace()
    {
        Cancel();
        Volatile.Write(ref _replaced, 1);
        if (Volatile.Read(ref _done) == 1) _cts.Dispose();
    }

    /// <summary>This write finished: dispose if it was already replaced.</summary>
    public void Finish()
    {
        Volatile.Write(ref _done, 1);
        if (Volatile.Read(ref _replaced) == 1) _cts.Dispose();
    }
}
