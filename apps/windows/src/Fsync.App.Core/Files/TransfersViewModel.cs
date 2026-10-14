using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Files;

/// <summary>
/// Pushes ("Send to", drag-drop) and pulls (accepted offers) with progress and cancel, plus
/// offers waiting for an answer. Pushed files go to Downloads\fsync, accepted automatically when
/// the setting is on and the offer is at most <see cref="AutoAcceptLimit"/>. UI thread only.
/// </summary>
public sealed partial class TransfersViewModel : ObservableObject, IDisposable
{
    /// <summary>Bigger offers always ask, even with auto-accept on (macOS: 1 GiB).</summary>
    public const ulong AutoAcceptLimit = 1UL << 30;
    private readonly EngineBridge _bridge;
    private readonly Func<string> _downloadDir;
    private readonly Func<bool> _autoAccept;
    private readonly TimeSpan _autoRetryDelay;
    /// <summary>Files of an offer whose pull already started (a second Accept skips them).</summary>
    private readonly Dictionary<string, HashSet<uint>> _started = [];
    private readonly HashSet<string> _accepting = [];

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
            AppLog.Info($"transfer {ev.transferId} {ev.direction} {ev.state} bytes={ev.bytesDone}");
        if (ev.state == TransferState.Failed && _pulls.Remove(ev.transferId, out var pull)) OnPullFailed(item, pull, ev.message);
        else if (ev.state is TransferState.Done or TransferState.Cancelled) _pulls.Remove(ev.transferId);
        OnPropertyChanged(nameof(ActiveCount));
    }

    internal void OnIncomingFiles(ClipItem offer)
    {
        var o = new IncomingOffer(offer);
        Offers.Insert(0, o);
        if (_autoAccept() && offer.totalSize <= AutoAcceptLimit) _ = AcceptAsync(o);
    }

    /// <summary>
    /// Pulls every file of the offer into the download folder. The offer stays listed until every
    /// pull has started, so a failure can be retried with Accept (started files are not pulled twice).
    /// </summary>
    [RelayCommand]
    public async Task AcceptAsync(IncomingOffer offer)
    {
        if (!_accepting.Add(offer.Item.itemId)) return; // a second click while the first one runs
        var started = _started.TryGetValue(offer.Item.itemId, out var s) ? s : _started[offer.Item.itemId] = [];
        try
        {
            Directory.CreateDirectory(_downloadDir());
            foreach (var f in offer.Item.files.Where(f => !started.Contains(f.fileId)))
            {
                await StartPullAsync(new Pull(offer.Item.itemId, f.fileId, f.name, 0));
                started.Add(f.fileId);
            }
            Offers.Remove(offer);
            _started.Remove(offer.Item.itemId);
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { Error = "Could not save files to the download folder."; }
        finally { _accepting.Remove(offer.Item.itemId); }
    }

    [RelayCommand]
    public void Decline(IncomingOffer offer)
    {
        Offers.Remove(offer);
        _started.Remove(offer.Item.itemId);
    }

    /// <summary>Pushes files/folders to one device. Returns false (and sets Error) on failure.</summary>
    public async Task<bool> SendAsync(string nodeId, string deviceName, IReadOnlyList<string> paths)
    {
        if (paths.Count == 0)
        {
            Error = "Nothing to send: drop files or folders.";
            return false;
        }
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
