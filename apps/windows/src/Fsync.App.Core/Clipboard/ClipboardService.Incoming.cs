using System.Collections.Concurrent;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Shell;

namespace Fsync.App.Clipboard;

/// <summary>Received clips → clipboard: one write at a time, a newer clip cancels an older one.</summary>
public sealed partial class ClipboardService
{
    private readonly SemaphoreSlim _inLane = new(1, 1);
    private long _inGen;
    private WriteTicket _inTicket = new();
    private ClipDataObject? _current;
    private volatile string? _currentItemId;
    /// <summary>Received clips of this session by item id (history "Copy"; core keeps their offers).</summary>
    private readonly ConcurrentDictionary<string, ClipItem> _recent = new();
    private const int RecentMax = 200;

    /// <summary>Raised on the clipboard thread when we put a received clip on the clipboard.</summary>
    public event Action<ClipDataObject>? Written;

    private void OnRemoteClip(ClipItem item)
    {
        if (item.push) return; // "Send to" offers arrive as IncomingFiles
        AppLog.Info($"clipboard: received {item.kind} reps={item.reps.Length} files={item.files.Length} bytes={item.totalSize} manual={IsManual}");
        Remember(item);
        if (IsManual)
        {
            Pending = item;
            LastActivity = $"{Kind(item)} from {item.fromName}: press {PasteHotKeyText} to paste";
            return;
        }
        Observe(WriteAsync(item), "write");
    }

    private void Remember(ClipItem item)
    {
        _recent[item.itemId] = item;
        if (_recent.Count <= RecentMax) return;
        foreach (var old in _recent.Values.OrderBy(i => i.tsMs).Take(_recent.Count - RecentMax).ToList())
            _recent.TryRemove(old.itemId, out _);
    }

    /// <summary>History "Copy": any item with content in core's history, or a files clip offered this session.</summary>
    public bool CanCopyBack(HistoryItem item) => item.kind != ClipKind.Files || _recent.ContainsKey(item.id);

    /// <summary>
    /// Puts a history item back on the clipboard. A clip received this session goes the normal way
    /// (files stay lazy, the peer still has the offer); anything else is read from core's history
    /// with its full content. Files-only items of an earlier session cannot be offered again.
    /// </summary>
    public async Task<bool> CopyBackAsync(HistoryItem item)
    {
        if (_recent.TryGetValue(item.id, out var recent)) return await WriteAsync(recent);
        return item.kind != ClipKind.Files && await WriteHistoryItemAsync(item);
    }

    /// <summary>Manual mode: puts the pending clip on the clipboard; it stays pending if that fails.</summary>
    public async Task<bool> PastePendingAsync()
    {
        if (Pending is not { } item) return false;
        var ok = await WriteAsync(item);
        if (ok) Post(() => { if (ReferenceEquals(Pending, item)) Pending = null; });
        return ok;
    }

    /// <summary>Token of the newest write (tests: an older write's downloads see it cancelled).</summary>
    internal CancellationToken LatestWriteToken => Volatile.Read(ref _inTicket).Token;

    /// <summary>Writes a received clip; returns false when superseded or failed.</summary>
    public Task<bool> WriteAsync(ClipItem item) =>
        WriteCoreAsync(item.itemId, $"{Kind(item)} from {item.fromName}", ct => BuildAsync(item, ct));

    private async Task<(List<ClipEntry> Entries, List<VirtualFile> Files)> BuildAsync(ClipItem item, CancellationToken ct)
    {
        if (EagerFiles.Applies(_prefs.FileStrategy, item))
        {
            // Off the caller thread: the in-flight mark waits for a running inbox clean, and a UI-initiated write must not.
            return ([IncomingClip.DropList(await Task.Run(() => DownloadAsync(item, ct)))], []);
        }
        return await Task.Run(() => IncomingClip.Build(item,
            idx => _gate.Run(() => _bridge.Call(e => e.FetchRep(item.itemId, idx))),
            (fileId, offset, len) => _gate.Run(() => _bridge.Call(e => e.ReadFileRange(item.itemId, fileId, (ulong)offset, (uint)len)))));
    }

    /// <summary>One write at a time; a newer write cancels an older one. <paramref name="build"/> makes the entries.</summary>
    private async Task<bool> WriteCoreAsync(string itemId, string activity,
        Func<CancellationToken, Task<(List<ClipEntry> Entries, List<VirtualFile> Files)>> build)
    {
        if (IsDisposed) return false; // quitting: nothing to write
        var gen = Interlocked.Increment(ref _inGen);
        var ticket = new WriteTicket();
        Interlocked.Exchange(ref _inTicket, ticket).Replace(); // a newer clip stops older downloads
        if (IsDisposed) { ticket.Replace(); ticket.Finish(); return false; } // Dispose ran between the check above and the swap
        try
        {
            await _inLane.WaitAsync(ticket.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            ticket.Finish();
            return false;
        }

        try
        {
            var (entries, files) = await build(ticket.Token);
            if (entries.Count == 0 && files.Count == 0)
            {
                Post(() => LastError = "Received a format this PC cannot paste");
                return false;
            }
            entries.AddRange(ClipEntry.PrivacyFlags());
            if (gen != Volatile.Read(ref _inGen)) return false; // a newer clip arrived meanwhile
            await _sta.InvokeAsync(() => SetCurrent(new ClipDataObject(entries, files), itemId));
            AppLog.Info($"clipboard: wrote formats={entries.Count} virtual files={files.Count}");
            Post(() => { LastActivity = activity; LastError = null; });
            return true;
        }
        catch (OperationCanceledException) { return false; }
        catch (FsyncException e)
        {
            AppLog.Warn($"clipboard: write failed: {e.GetType().Name}");
            Post(() => LastError = UserMessages.For(e));
            return false;
        }
        catch (Exception e)
        {
            AppLog.Warn($"clipboard: write failed: {e.GetType().Name}");
            Post(() => LastError = e is TimeoutException ? "The files took too long to download." : "Could not write the clipboard.");
            return false;
        }
        finally
        {
            _inLane.Release();
            ticket.Finish();
        }
    }

    /// <summary>On the clipboard thread: replaces our entry; the old one forgets its streams.</summary>
    private void SetCurrent(ClipDataObject obj, string itemId)
    {
        obj.RenderFailed += (_, e) => AppLog.Warn($"clipboard: delayed render failed: {e.GetType().Name}");
        obj.RenderFailed += (_, e) => Post(() => LastError = e is FsyncException f ? UserMessages.For(f) : "Could not fetch the clip");
        OleClipboard.Set(obj);
        _current?.ReleaseStreams();
        _current = obj;
        _currentItemId = itemId;
        Written?.Invoke(obj);
    }

    private static string Kind(ClipItem i) => i.kind switch
    {
        ClipKind.Text => "Text",
        ClipKind.Rich => "Formatted text",
        ClipKind.Image => "Image",
        _ => i.files.Length == 1 ? "1 file" : $"{i.files.Length} files",
    };
}
