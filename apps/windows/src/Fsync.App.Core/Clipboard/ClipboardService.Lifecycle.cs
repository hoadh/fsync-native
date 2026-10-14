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

    private bool IsDisposed => Volatile.Read(ref _disposed) == 1;

    /// <summary>Timer.Change that is a no-op once the service is disposed (wndproc, settings refresh).</summary>
    private void Schedule(Timer timer, TimeSpan due, TimeSpan period)
    {
        if (IsDisposed) return;
        try { timer.Change(due, period); }
        catch (ObjectDisposedException) { } // raced with Dispose
    }

    /// <summary>
    /// Inbox follows the history retention (days, bytes), like macOS. Kept: the clip on the
    /// clipboard and every folder a download is writing into (plus anything written in the last
    /// few minutes, InboxDirectory.MinAge). Cleans are serialized by InboxDirectory.
    /// </summary>
    internal int CleanInbox()
    {
        if (_settings is not { } s) return 0;
        try
        {
            var keep = new HashSet<string>(_inFlight.Keys, StringComparer.OrdinalIgnoreCase);
            if (_currentItemId is { } id) keep.Add(id);
            var removed = _inbox.Clean(s.retentionDays, s.retentionBytes, keep);
            if (removed > 0) AppLog.Info($"clipboard: inbox cleanup removed {removed} folder(s)");
            return removed;
        }
        catch (Exception e)
        {
            AppLog.Warn($"clipboard: inbox cleanup failed: {e.GetType().Name}");
            return 0;
        }
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
    /// downloads cancelled, the listener and our entry released, then running calls (Explorer
    /// reads, renders) are waited for so the engine can be disposed after this returns. Call it off
    /// the UI thread (App.Quit does).
    /// </summary>
    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) == 1) return;
        var deadline = DateTime.UtcNow + QuitBudget;
        _bridge.RemoteClip -= OnRemoteClip;
        _bridge.Transfer -= OnTransfer;
        _gate.Close();
        _inTicket.Cancel();
        try
        {
            // Listener first: its wndproc must not touch the settle timer after it is disposed.
            if (!_sta.TryInvoke(ReleaseClipboard, Left(deadline))) AppLog.Warn("clipboard: release on exit timed out");
        }
        catch (Exception e) { AppLog.Warn($"clipboard: release on exit failed: {e.GetType().Name}"); }
        _settle.Dispose();
        _cleanup.Dispose();
        if (!_gate.WaitIdle(Left(deadline))) AppLog.Warn($"clipboard: {_gate.InFlight} engine call(s) still running at quit");
        _sta.Dispose(TimeSpan.FromMilliseconds(500));
    }

    private static TimeSpan Left(DateTime deadline)
    {
        var left = deadline - DateTime.UtcNow;
        return left > TimeSpan.Zero ? left : TimeSpan.Zero;
    }
}
