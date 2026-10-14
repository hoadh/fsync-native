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
    /// <summary>Item folders being downloaded into (inbox cleanup keeps them), with a use count.</summary>
    private readonly ConcurrentDictionary<string, int> _inFlight = new(StringComparer.OrdinalIgnoreCase);
    private static readonly TimeSpan EarlyKeep = TimeSpan.FromSeconds(60);

    /// <summary>How long a paste may wait for its downloads (tests shorten it).</summary>
    internal TimeSpan EagerTimeout { get; set; } = EagerFiles.Timeout;
    /// <summary>Transfers this service cancelled (diagnostics, tests).</summary>
    internal ConcurrentQueue<ulong> CancelledTransfers { get; } = new();

    /// <summary>
    /// Downloads the clip's files into inbox\&lt;item&gt; (a file already there with the right size is
    /// reused) and returns the top-level paths for CF_HDROP. One budget (<see cref="EagerTimeout"/>)
    /// and one cancellation (<paramref name="ct"/>: newer clip, quit) cover starting the transfers
    /// (fetch_file_to can block through the dial) and waiting for them; every transfer that started
    /// and did not finish is cancelled on the way out, whatever ended the download. The folder is
    /// marked in flight (before it exists) so the inbox cleanup leaves it alone.
    /// </summary>
    internal async Task<List<string>> DownloadAsync(ClipItem item, CancellationToken ct)
    {
        if (!Files.InboxDirectory.IsValidItemId(item.itemId)) throw new ArgumentException("invalid item id");
        using var inFlight = BeginInFlight(item.itemId);
        var dir = _inbox.Folder(item.itemId);
        _inbox.Touch(dir); // a reused folder is fresh again for the cleanup's minimum age
        using var budget = CancellationTokenSource.CreateLinkedTokenSource(ct);
        // A spent budget must be spent now: CancelAfter(0) fires on a timer, so a start could slip through before it.
        if (EagerTimeout != Timeout.InfiniteTimeSpan && EagerTimeout <= TimeSpan.Zero) budget.Cancel(); else budget.CancelAfter(EagerTimeout);
        var ids = new List<ulong>();
        var waits = new List<Task<string>>();
        try
        {
            foreach (var f in item.files)
            {
                budget.Token.ThrowIfCancellationRequested();
                if (EagerFiles.Existing(dir, f) is not null) continue;
                var start = _gate.RunAsync(() => _bridge.RunAsync(e => e.FetchFileTo(item.itemId, f.fileId, dir)));
                try { ids.Add(await start.WaitAsync(budget.Token)); }
                catch (OperationCanceledException)
                {
                    CancelOnceStarted(start); // we stop waiting; the transfer must not run on unattended
                    throw;
                }
                waits.Add(Register(ids[^1])); // at once: its outcome is kept and the finally can cancel it
            }
            if (waits.Count > 0) Post(() => LastActivity = $"Downloading {waits.Count} file(s) from {item.fromName}");
            await Task.WhenAll(waits).WaitAsync(budget.Token);
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            throw new TimeoutException("eager download timed out");
        }
        finally
        {
            foreach (var id in ids.Where(id => _downloads.TryRemove(id, out _))) await CancelTransferAsync(id);
        }
        var tops = EagerFiles.TopLevel(dir, item.files);
        return tops.Count > 0 ? tops : throw new InvalidDataException("no usable file names in the clip");
    }

    private void CancelOnceStarted(Task<ulong> start) =>
        start.ContinueWith(t => CancelTransferAsync(t.Result), CancellationToken.None,
            TaskContinuationOptions.OnlyOnRanToCompletion, TaskScheduler.Default);

    private async Task CancelTransferAsync(ulong id)
    {
        CancelledTransfers.Enqueue(id);
        try { await _bridge.RunAsync(e => e.Cancel(id)); }
        catch (Exception e) { AppLog.Warn($"clipboard: cancel transfer {id}: {e.GetType().Name}"); }
    }

    /// <summary>The outcome of transfer <paramref name="id"/>, also when it already arrived.</summary>
    internal Task<string> Register(ulong id)
    {
        var tcs = _downloads.GetOrAdd(id, _ => new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously));
        if (_early.TryRemove(id, out var early)) Complete(early.Ev);
        return tcs.Task;
    }

    internal void OnTransfer(TransferEvent ev)
    {
        if (ev.direction != TransferDirection.Receive || ev.state is TransferState.Started or TransferState.Progress) return;
        if (_downloads.ContainsKey(ev.transferId)) { Complete(ev); return; }
        var now = DateTime.UtcNow;
        foreach (var old in _early.Where(kv => now - kv.Value.At > EarlyKeep).Select(kv => kv.Key).ToList()) _early.TryRemove(old, out _);
        _early[ev.transferId] = (ev, now);
        // Registered meanwhile (between the check and the store): hand it over now, or it is lost.
        if (_downloads.ContainsKey(ev.transferId) && _early.TryRemove(ev.transferId, out var e2)) Complete(e2.Ev);
    }

    private void Complete(TransferEvent ev)
    {
        if (!_downloads.TryRemove(ev.transferId, out var tcs)) return;
        if (ev.state == TransferState.Done && ev.path is { } p) tcs.TrySetResult(p);
        else tcs.TrySetException(new IOException(ev.message ?? $"download {ev.state.ToString().ToLowerInvariant()}"));
    }

    /// <summary>Marks an item folder as being written; dispose to release.</summary>
    internal IDisposable BeginInFlight(string itemId)
    {
        // Under the cleanup lock: a clean that is running finishes first, a later one sees this mark.
        _inbox.Exclusive(() => _inFlight.AddOrUpdate(itemId, 1, (_, n) => n + 1));
        return new Release(() =>
        {
            if (_inFlight.AddOrUpdate(itemId, 0, (_, n) => n - 1) <= 0) _inFlight.TryRemove(new KeyValuePair<string, int>(itemId, 0));
        });
    }

    private sealed class Release(Action a) : IDisposable
    {
        private int _done;
        public void Dispose() { if (Interlocked.Exchange(ref _done, 1) == 0) a(); }
    }
}
