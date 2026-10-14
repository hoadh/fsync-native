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
    private CancellationTokenSource _inCts = new();
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

    public bool CanCopyBack(string itemId) => _recent.ContainsKey(itemId);

    /// <summary>Puts a clip received earlier in this session back on the clipboard.</summary>
    public async Task<bool> CopyBackAsync(string itemId)
    {
        if (!_recent.TryGetValue(itemId, out var item)) return false;
        await WriteAsync(item);
        return true;
    }

    /// <summary>Manual mode: puts the pending clip on the clipboard.</summary>
    public async Task PastePendingAsync()
    {
        if (Pending is not { } item) return;
        Pending = null;
        await WriteAsync(item);
    }

    /// <summary>Writes a received clip (also history "Copy"); returns false when superseded or failed.</summary>
    public async Task<bool> WriteAsync(ClipItem item)
    {
        var gen = Interlocked.Increment(ref _inGen);
        var cts = new CancellationTokenSource();
        Interlocked.Exchange(ref _inCts, cts).Cancel(); // a newer clip stops older downloads
        try
        {
            await _inLane.WaitAsync(cts.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) { return false; }
        try
        {
            List<ClipEntry> entries;
            List<VirtualFile> files;
            if (EagerFiles.Applies(_prefs.FileStrategy, item))
            {
                entries = [IncomingClip.DropList(await DownloadAsync(item, cts.Token))];
                files = [];
            }
            else
            {
                (entries, files) = await Task.Run(() => IncomingClip.Build(item,
                    idx => _gate.Run(() => _bridge.Call(e => e.FetchRep(item.itemId, idx))),
                    (fileId, offset, len) => _gate.Run(() => _bridge.Call(e => e.ReadFileRange(item.itemId, fileId, (ulong)offset, (uint)len)))));
            }
            if (entries.Count == 0 && files.Count == 0)
            {
                Post(() => LastError = "Received a format this PC cannot paste");
                return false;
            }
            entries.AddRange(ClipEntry.PrivacyFlags());
            if (gen != Volatile.Read(ref _inGen)) return false; // a newer clip arrived meanwhile
            await _sta.InvokeAsync(() => SetCurrent(new ClipDataObject(entries, files), item.itemId));
            AppLog.Info($"clipboard: wrote formats={entries.Count} virtual files={files.Count}");
            Post(() => { LastActivity = $"{Kind(item)} from {item.fromName}"; LastError = null; });
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
        finally { _inLane.Release(); }
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
