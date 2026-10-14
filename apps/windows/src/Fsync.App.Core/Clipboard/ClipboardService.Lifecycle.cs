using Fsync.App.Services;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>Inbox retention, exit policy and a bounded shutdown.</summary>
public sealed partial class ClipboardService
{
    /// <summary>Quit waits at most this long for clipboard work (core teardown itself takes up to ~5 s).</summary>
    public static readonly TimeSpan QuitBudget = TimeSpan.FromSeconds(3);
    private static readonly TimeSpan CleanupInterval = TimeSpan.FromHours(24);
    private readonly Timer _cleanup;
    private int _disposed;

    /// <summary>Inbox follows the history retention (days, bytes), like macOS; the current clip is kept.</summary>
    internal void CleanInbox()
    {
        if (_settings is not { } s) return;
        try
        {
            var keep = _currentItemId is { } id ? new[] { id } : [];
            var removed = _inbox.Clean(s.retentionDays, s.retentionBytes, keep);
            if (removed > 0) AppLog.Info($"clipboard: inbox cleanup removed {removed} folder(s)");
        }
        catch (Exception e) { AppLog.Warn($"clipboard: inbox cleanup failed: {e.GetType().Name}"); }
    }

    /// <summary>
    /// Exit policy (ADR 013 §5): an entry with virtual files or with formats never rendered is
    /// cleared (no network at exit); a fully rendered entry is flushed so the clip survives.
    /// </summary>
    private void ReleaseClipboard()
    {
        _listener?.Dispose();
        _listener = null;
        if (_current is not { } obj) return;
        _current = null;
        if (!OleClipboard.IsCurrent(obj)) return;
        if (obj.HasVirtualFiles || obj.HasPendingLazy)
        {
            obj.Abort();
            OleClipboard.ClearIfCurrent(obj);
        }
        else OleClipboard.Flush();
    }

    /// <summary>
    /// Stops clipboard work within <see cref="QuitBudget"/>: newer engine calls are refused, running
    /// downloads cancelled, the clipboard released, then running calls (Explorer reads, renders) are
    /// waited for so the engine can be disposed after this returns.
    /// </summary>
    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) == 1) return;
        var deadline = DateTime.UtcNow + QuitBudget;
        _bridge.RemoteClip -= OnRemoteClip;
        _bridge.Transfer -= OnTransfer;
        _settle.Dispose();
        _cleanup.Dispose();
        _gate.Close();
        _inCts.Cancel();
        try
        {
            if (!_sta.TryInvoke(ReleaseClipboard, Left(deadline))) AppLog.Warn("clipboard: release on exit timed out");
        }
        catch (Exception e) { AppLog.Warn($"clipboard: release on exit failed: {e.GetType().Name}"); }
        if (!_gate.WaitIdle(Left(deadline))) AppLog.Warn($"clipboard: {_gate.InFlight} engine call(s) still running at quit");
        _sta.Dispose(TimeSpan.FromMilliseconds(500));
    }

    private static TimeSpan Left(DateTime deadline)
    {
        var left = deadline - DateTime.UtcNow;
        return left > TimeSpan.Zero ? left : TimeSpan.Zero;
    }
}
