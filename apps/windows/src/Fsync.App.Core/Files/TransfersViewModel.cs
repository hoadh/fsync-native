using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Files;

/// <summary>
/// Pushes ("Send to", drag-drop) and pulls (accepted offers) with progress and cancel, plus
/// offers waiting for an answer. Pushed files go to Downloads\fsync, auto-accepted when the
/// setting is on. Lives as long as the app; UI thread only.
/// </summary>
public sealed partial class TransfersViewModel : ObservableObject, IDisposable
{
    private readonly EngineBridge _bridge;
    private readonly Func<string> _downloadDir;
    private readonly Func<bool> _autoAccept;
    private readonly TimeSpan _autoRetryDelay;
    private readonly Dictionary<ulong, Pull> _pulls = [];
    private readonly Dictionary<TransferItem, Pull> _failed = [];

    /// <summary>What a pull needs to start again.</summary>
    private sealed record Pull(string ItemId, uint FileId, string Name, int Attempt);

    public ObservableCollection<TransferItem> Transfers { get; } = [];
    public ObservableCollection<IncomingOffer> Offers { get; } = [];
    [ObservableProperty] private string? _error;

    /// <param name="downloadDir">Where accepted files go (created on demand).</param>
    /// <param name="autoAccept">Current core setting auto_accept_files.</param>
    /// <param name="autoRetryDelay">A pull that failed on the network is retried once after this (default 20 s).</param>
    public TransfersViewModel(EngineBridge bridge, Func<string> downloadDir, Func<bool> autoAccept, TimeSpan? autoRetryDelay = null)
    {
        _autoRetryDelay = autoRetryDelay ?? TimeSpan.FromSeconds(20);
        _bridge = bridge;
        _downloadDir = downloadDir;
        _autoAccept = autoAccept;
        _bridge.Transfer += OnTransfer;
        _bridge.IncomingFiles += OnIncomingFiles;
    }

    public int ActiveCount => Transfers.Count(t => t.IsActive);

    private void OnTransfer(TransferEvent ev)
    {
        var item = Transfers.FirstOrDefault(t => t.Id == ev.transferId);
        if (item is null)
        {
            // A pull started elsewhere (eager paste) or an event before our row: show it too.
            item = new TransferItem(ev.transferId, ev.path is { } p ? Path.GetFileName(p) : "File", ev.direction);
            Transfers.Insert(0, item);
        }
        item.Apply(ev);
        if (ev.state is TransferState.Failed or TransferState.Done or TransferState.Cancelled)
            Services.AppLog.Info($"transfer {ev.transferId} {ev.direction} {ev.state} bytes={ev.bytesDone}{(ev.state == TransferState.Failed ? $" ({ev.message})" : "")}");
        if (ev.state == TransferState.Failed && _pulls.Remove(ev.transferId, out var pull)) OnPullFailed(item, pull, ev.message);
        else if (ev.state is TransferState.Done or TransferState.Cancelled) _pulls.Remove(ev.transferId);
        OnPropertyChanged(nameof(ActiveCount));
    }

    private void OnIncomingFiles(ClipItem offer)
    {
        var o = new IncomingOffer(offer);
        if (_autoAccept()) _ = AcceptAsync(o);
        else Offers.Insert(0, o);
    }

    /// <summary>Pulls every file of the offer into the download folder.</summary>
    [RelayCommand]
    public async Task AcceptAsync(IncomingOffer offer)
    {
        Offers.Remove(offer);
        var dir = _downloadDir();
        try
        {
            Directory.CreateDirectory(dir);
            foreach (var f in offer.Item.files) await StartPullAsync(new Pull(offer.Item.itemId, f.fileId, f.name, 0));
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { Error = $"Could not save files: {e.Message}"; }
    }

    private async Task StartPullAsync(Pull p)
    {
        var id = await _bridge.RunAsync(e => e.FetchFileTo(p.ItemId, p.FileId, _downloadDir()));
        _pulls[id] = p;
        Services.AppLog.Info($"transfer {id}: pull started (attempt {p.Attempt}), rows={Transfers.Count}");
        Track(id, p.Name, TransferDirection.Receive);
        // fetch_file_to may return only after the dial: the outcome can already be here.
        if (Transfers.FirstOrDefault(t => t.Id == id) is { IsActive: false } row && _pulls.Remove(id) && row.State == TransferState.Failed)
            OnPullFailed(row, p, row.FailureMessage);
    }

    /// <summary>
    /// A failed pull gets a Retry button. A network failure is retried once by itself: right after
    /// the sender restarted, dials can time out for ~30 s (stale path to the same node id).
    /// </summary>
    private void OnPullFailed(TransferItem row, Pull p, string? message)
    {
        Services.AppLog.Info($"transfer {row.Id}: pull failed (attempt {p.Attempt}): {message}");
        _failed[row] = p;
        row.CanRetry = true;
        if (p.Attempt == 0 && message?.StartsWith("network", StringComparison.OrdinalIgnoreCase) == true)
        {
            row.Detail += $" (trying again in {_autoRetryDelay.TotalSeconds:0} s)";
            _ = RetryLaterAsync(row);
        }
    }

    private async Task RetryLaterAsync(TransferItem row)
    {
        await Task.Delay(_autoRetryDelay);
        if (_failed.ContainsKey(row)) await RetryAsync(row);
    }

    /// <summary>Starts a failed pull again (the old row is replaced by the new transfer).</summary>
    [RelayCommand]
    public async Task RetryAsync(TransferItem row)
    {
        if (!_failed.Remove(row, out var p)) return;
        Transfers.Remove(row);
        try
        {
            await StartPullAsync(p with { Attempt = p.Attempt + 1 });
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    [RelayCommand]
    public void Decline(IncomingOffer offer) => Offers.Remove(offer);

    /// <summary>Pushes files/folders to one device. Returns false (and sets Error) on failure.</summary>
    public async Task<bool> SendAsync(string nodeId, string deviceName, IReadOnlyList<string> paths)
    {
        if (paths.Count == 0) return false;
        try
        {
            var id = await _bridge.RunAsync(e => e.SendFiles(nodeId, [.. paths]));
            var name = paths.Count == 1 ? Path.GetFileName(paths[0].TrimEnd('\\')) : $"{paths.Count} items";
            Track(id, $"{name} to {deviceName}", TransferDirection.Send);
            Error = null;
            return true;
        }
        catch (FsyncException e)
        {
            Error = UserMessages.For(e);
            return false;
        }
    }

    [RelayCommand]
    public async Task CancelAsync(TransferItem item)
    {
        try { await _bridge.RunAsync(e => e.Cancel(item.Id)); }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    [RelayCommand]
    public void ClearFinished()
    {
        foreach (var t in Transfers.Where(t => !t.IsActive).ToList()) Transfers.Remove(t);
    }

    private void Track(ulong id, string title, TransferDirection dir)
    {
        if (Transfers.FirstOrDefault(t => t.Id == id) is { } early)
        {
            early.Title = title; // events were faster than the call's return
            return;
        }
        Transfers.Insert(0, new TransferItem(id, title, dir));
        OnPropertyChanged(nameof(ActiveCount));
    }

    public void Dispose()
    {
        _bridge.Transfer -= OnTransfer;
        _bridge.IncomingFiles -= OnIncomingFiles;
    }
}
