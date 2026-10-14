using System.Collections.Concurrent;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Clipboard;

/// <summary>Eager strategy downloads: bounded wait, cancellation, reuse of an earlier copy.</summary>
public sealed partial class ClipboardService
{
    private readonly ConcurrentDictionary<ulong, TaskCompletionSource<string>> _downloads = new();
    /// <summary>Outcomes that arrived before their id was registered (fetch_file_to may return after the dial).</summary>
    private readonly ConcurrentDictionary<ulong, (TransferEvent Ev, DateTime At)> _early = new();
    private static readonly TimeSpan EarlyKeep = TimeSpan.FromSeconds(60);

    /// <summary>
    /// Downloads the clip's files into inbox\&lt;item&gt; (a file already there with the right size is
    /// reused) and returns the top-level paths for CF_HDROP. Gives up after <see cref="EagerFiles.Timeout"/>
    /// or when <paramref name="ct"/> is cancelled (newer clip, quit); started transfers are cancelled.
    /// </summary>
    internal async Task<List<string>> DownloadAsync(ClipItem item, CancellationToken ct)
    {
        var dir = _inbox.Folder(item.itemId);
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(ct);
        timeout.CancelAfter(EagerFiles.Timeout);
        var started = new List<ulong>();
        var waits = new List<Task<string>>();
        try
        {
            foreach (var f in item.files)
            {
                timeout.Token.ThrowIfCancellationRequested();
                if (EagerFiles.Existing(dir, f) is not null) continue;
                var id = await _gate.RunAsync(() => _bridge.RunAsync(e => e.FetchFileTo(item.itemId, f.fileId, dir)));
                started.Add(id);
                var tcs = _downloads.GetOrAdd(id, _ => new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously));
                if (_early.TryRemove(id, out var early)) Complete(early.Ev);
                waits.Add(tcs.Task);
            }
            if (waits.Count > 0) Post(() => LastActivity = $"Downloading {waits.Count} file(s) from {item.fromName}");
            await Task.WhenAll(waits).WaitAsync(timeout.Token);
            return EagerFiles.TopLevel(dir, item.files);
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            throw new TimeoutException("eager download timed out");
        }
        finally
        {
            foreach (var id in started.Where(id => _downloads.TryRemove(id, out _)))
            {
                try { await _bridge.RunAsync(e => e.Cancel(id)); }
                catch (Exception e) { AppLog.Warn($"clipboard: cancel transfer {id}: {e.GetType().Name}"); }
            }
        }
    }

    private void OnTransfer(TransferEvent ev)
    {
        if (ev.direction != TransferDirection.Receive || ev.state is TransferState.Started or TransferState.Progress) return;
        if (_downloads.ContainsKey(ev.transferId)) { Complete(ev); return; }
        var now = DateTime.UtcNow;
        foreach (var old in _early.Where(kv => now - kv.Value.At > EarlyKeep).Select(kv => kv.Key).ToList()) _early.TryRemove(old, out _);
        _early[ev.transferId] = (ev, now);
    }

    private void Complete(TransferEvent ev)
    {
        if (!_downloads.TryRemove(ev.transferId, out var tcs)) return;
        if (ev.state == TransferState.Done && ev.path is { } p) tcs.TrySetResult(p);
        else tcs.TrySetException(new IOException(ev.message ?? $"download {ev.state.ToString().ToLowerInvariant()}"));
    }
}
