using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>One history row: preview text, who, when, size; pin state is live.</summary>
public sealed partial class HistoryRow(HistoryItem item, string peerName, bool canCopy) : ObservableObject
{
    public HistoryItem Item { get; } = item;
    public string Preview => Item.preview is { Length: > 0 } p ? p.ReplaceLineEndings(" ") : Item.kind switch
    {
        ClipKind.Image => "Image",
        ClipKind.Files => "Files",
        _ => "(no preview)",
    };
    public string Meta => $"{(Item.outgoing ? "Copied here" : $"From {peerName}")} · {When(Item.createdMs)} · {Files.TransferItem.Size(Item.totalSize)}";
    public bool CanCopy { get; } = canCopy;
    [ObservableProperty] private bool _pinned = item.pinned;

    internal static string When(ulong ms)
    {
        var t = DateTimeOffset.FromUnixTimeMilliseconds((long)ms).ToLocalTime();
        return t.Date == DateTimeOffset.Now.Date ? t.ToString("t") : t.ToString("g");
    }

    /// <summary>Accessible name.</summary>
    public override string ToString() => $"{Preview}, {Meta}{(Pinned ? ", pinned" : "")}";
}

/// <summary>
/// Clipboard history window: search (core FTS over previews), paging, pin, delete, clear, and
/// copy-back with the item's full content (core history_item_reps; files only for clips offered this session).
/// </summary>
public sealed partial class HistoryViewModel(EngineBridge bridge, ClipboardService clipboard) : ObservableObject
{
    public const uint PageSize = 50;
    /// <summary>Typing pause before a search runs.</summary>
    public static readonly TimeSpan SearchDelay = TimeSpan.FromMilliseconds(300);
    private string? _cursor;
    private int _version; // a newer refresh makes older results stale
    private CancellationTokenSource? _typing;
    private Dictionary<string, string> _names = [];

    public ObservableCollection<HistoryRow> Items { get; } = [];
    [ObservableProperty] private string _query = "";
    [ObservableProperty] private bool _hasMore;
    [ObservableProperty] private string? _error;
    [ObservableProperty] private string? _status;

    [RelayCommand]
    public async Task RefreshAsync()
    {
        var v = ++_version;
        RefreshCount++;
        try
        {
            _names = (await bridge.RunAsync(e => e.Devices())).ToDictionary(d => d.nodeId, d => d.name);
        }
        catch (FsyncException) { } // names are cosmetic; offline is fine
        if (v != _version) return;
        _cursor = null;
        Items.Clear();
        await LoadPageAsync(v);
    }

    /// <summary>Refreshes started (tests: the search debounce).</summary>
    internal int RefreshCount { get; private set; }

    [RelayCommand]
    public Task LoadMoreAsync() => LoadPageAsync(_version);

    private async Task LoadPageAsync(int v)
    {
        var q = string.IsNullOrWhiteSpace(Query) ? null : Query.Trim();
        try
        {
            var cursor = _cursor;
            var page = await bridge.RunAsync(e => e.History(q, cursor, PageSize));
            if (v != _version) return; // the query changed while this page loaded
            foreach (var i in page.items)
                Items.Add(new HistoryRow(i, i.peer is { } p && _names.TryGetValue(p, out var n) ? n : "another device", clipboard.CanCopyBack(i)));
            _cursor = page.nextCursor;
            HasMore = _cursor is not null;
            Error = null;
            Status = Items.Count == 0 ? (q is null ? "Nothing copied yet." : "No matches.") : null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    partial void OnQueryChanged(string value)
    {
        _typing?.Cancel();
        var cts = _typing = new CancellationTokenSource();
        _ = SearchLaterAsync(cts.Token);
    }

    private async Task SearchLaterAsync(CancellationToken ct)
    {
        try
        {
            await Task.Delay(SearchDelay, ct);
            await RefreshAsync();
        }
        catch (OperationCanceledException) { } // more typing
        catch (Exception e) { Error = e is FsyncException f ? UserMessages.For(f) : "Search failed."; }
    }

    [RelayCommand]
    public async Task TogglePinAsync(HistoryRow row)
    {
        var pin = !row.Pinned;
        try
        {
            await bridge.RunAsync(e => e.Pin(row.Item.id, pin));
            row.Pinned = pin;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    [RelayCommand]
    public async Task DeleteAsync(HistoryRow row)
    {
        try
        {
            await bridge.RunAsync(e => e.Delete(row.Item.id));
            Items.Remove(row);
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    /// <summary>Removes everything (optionally keeping pinned items). The view asks first.</summary>
    public async Task ClearAsync(bool keepPinned)
    {
        try
        {
            await bridge.RunAsync(e => e.Clear(keepPinned));
            await RefreshAsync();
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    [RelayCommand]
    public async Task CopyAsync(HistoryRow row)
    {
        if (!await clipboard.CopyBackAsync(row.Item)) Error = "This item cannot be copied back (its files are no longer offered, or its content is gone).";
        else Status = "Copied to the clipboard.";
    }
}
