namespace Fsync.App.Clipboard;

/// <summary>
/// Counts engine calls made for the clipboard (delayed renders, Explorer reads, eager downloads,
/// publishes) so quit can wait for them, bounded, before the engine is disposed. After
/// <see cref="Close"/> new calls fail with <see cref="OperationCanceledException"/>.
/// </summary>
public sealed class EngineCallGate
{
    private readonly object _lock = new();
    private int _inFlight;
    private bool _closed;

    public int InFlight { get { lock (_lock) return _inFlight; } }
    public bool IsClosed { get { lock (_lock) return _closed; } }

    public T Run<T>(Func<T> call)
    {
        Enter();
        try { return call(); }
        finally { Exit(); }
    }

    public async Task<T> RunAsync<T>(Func<Task<T>> call)
    {
        Enter();
        try { return await call().ConfigureAwait(false); }
        finally { Exit(); }
    }

    private void Enter()
    {
        lock (_lock)
        {
            if (_closed) throw new OperationCanceledException("fsync is closing");
            _inFlight++;
        }
    }

    private void Exit()
    {
        lock (_lock)
        {
            _inFlight--;
            Monitor.PulseAll(_lock);
        }
    }

    /// <summary>Refuses new calls (idempotent).</summary>
    public void Close()
    {
        lock (_lock) _closed = true;
    }

    /// <summary>Waits until no call is running, at most <paramref name="timeout"/>; false on timeout.</summary>
    public bool WaitIdle(TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow + timeout;
        lock (_lock)
        {
            while (_inFlight > 0)
            {
                var left = deadline - DateTime.UtcNow;
                if (left <= TimeSpan.Zero) return false;
                Monitor.Wait(_lock, left);
            }
            return true;
        }
    }
}
