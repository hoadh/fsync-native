using System.Collections.Concurrent;
using Fsync.App.Bridge;
using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>Received clips → clipboard; eager file downloads; exit policy.</summary>
public sealed partial class ClipboardService
{
    /// <summary>On exit, unrendered lazy formats up to this size are fetched and flushed; more is cleared.</summary>
    public const long FlushOnExitBytes = 20 * 1024 * 1024;

    private readonly ConcurrentDictionary<ulong, TaskCompletionSource<string>> _downloads = new();
    private readonly ConcurrentDictionary<ulong, TransferEvent> _finished = new();
    private ClipDataObject? _current;
    /// <summary>Received clips of this session by item id (history "Copy"; core keeps their offers).</summary>
    private readonly ConcurrentDictionary<string, ClipItem> _recent = new();
    private const int RecentMax = 200;

    /// <summary>Raised on the clipboard thread when we put a received clip on the clipboard.</summary>
    public event Action<ClipDataObject>? Written;

    private void OnRemoteClip(ClipItem item)
    {
        if (item.push) return; // "Send to" offers arrive as IncomingFiles
        Remember(item);
        if (IsManual)
        {
            Pending = item;
            LastActivity = $"{Kind(item)} from {item.fromName}: press Ctrl+Alt+V to paste";
            return;
        }
        _ = WriteAsync(item);
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

    /// <summary>Writes a received clip (also used by history "Copy").</summary>
    public async Task WriteAsync(ClipItem item)
    {
        try
        {
            var (entries, files) = item.files.Length > 0 && _prefs.FileStrategy == FileClipStrategy.Eager
                ? ([IncomingClip.DropList(await DownloadAsync(item))], [])
                : await Task.Run(() => IncomingClip.Build(item,
                    idx => _bridge.Call(e => e.FetchRep(item.itemId, idx)),
                    (fileId, offset, len) => _bridge.Call(e => e.ReadFileRange(item.itemId, fileId, (ulong)offset, (uint)len))));
            if (entries.Count == 0 && files.Count == 0)
            {
                Post(() => LastError = "Received a format this PC cannot paste");
                return;
            }
            await _sta.InvokeAsync(() =>
            {
                var obj = new ClipDataObject(entries, files);
                obj.RenderFailed += (_, e) => Post(() => LastError = e is FsyncException f ? UserMessages.For(f) : "Could not fetch the clip");
                OleClipboard.Set(obj);
                _current = obj;
                Written?.Invoke(obj);
            });
            Post(() => { LastActivity = $"{Kind(item)} from {item.fromName}"; LastError = null; });
        }
        catch (FsyncException e) { Post(() => LastError = UserMessages.For(e)); }
        catch (Exception e) when (e is System.Runtime.InteropServices.COMException or IOException or InvalidOperationException)
        {
            Post(() => LastError = $"Could not write the clipboard: {e.Message}");
        }
    }

    /// <summary>Eager strategy: downloads every file into inbox\&lt;item&gt; and returns the final paths.</summary>
    private async Task<List<string>> DownloadAsync(ClipItem item)
    {
        var dir = Path.Combine(_inboxDir, item.itemId);
        Directory.CreateDirectory(dir);
        var waits = new List<Task<string>>();
        foreach (var f in item.files)
        {
            var id = await _bridge.RunAsync(e => e.FetchFileTo(item.itemId, f.fileId, dir));
            var tcs = _downloads.GetOrAdd(id, _ => new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously));
            if (_finished.TryRemove(id, out var early)) Complete(early); // finished before we registered
            waits.Add(tcs.Task);
        }
        Post(() => LastActivity = $"Downloading {item.files.Length} file(s) from {item.fromName}");
        return [.. await Task.WhenAll(waits)];
    }

    private void OnTransfer(TransferEvent ev)
    {
        if (ev.direction != TransferDirection.Receive || ev.state is TransferState.Started or TransferState.Progress) return;
        if (!_downloads.ContainsKey(ev.transferId)) _finished[ev.transferId] = ev;
        Complete(ev);
    }

    private void Complete(TransferEvent ev)
    {
        if (!_downloads.TryRemove(ev.transferId, out var tcs)) return;
        if (ev.state == TransferState.Done && ev.path is { } p) tcs.TrySetResult(p);
        else tcs.TrySetException(new IOException(ev.message ?? $"download {ev.state.ToString().ToLowerInvariant()}"));
        _finished.TryRemove(ev.transferId, out _);
    }

    /// <summary>
    /// Exit policy (ADR 010 decision 4): never flush virtual files (FileContents would be lost);
    /// small lazy formats are rendered and flushed so the clip survives; anything bigger is cleared.
    /// </summary>
    private void ReleaseClipboard()
    {
        _listener?.Dispose();
        _listener = null;
        if (_current is not { } obj || !OleClipboard.IsCurrent(obj)) return;
        if (obj.HasVirtualFiles || obj.PendingLazyBytes > FlushOnExitBytes)
        {
            obj.Abort();
            OleClipboard.ClearIfCurrent(obj);
        }
        else OleClipboard.Flush();
        _current = null;
    }

    private static string Kind(ClipItem i) => i.kind switch
    {
        ClipKind.Text => "Text",
        ClipKind.Rich => "Formatted text",
        ClipKind.Image => "Image",
        _ => i.files.Length == 1 ? "1 file" : $"{i.files.Length} files",
    };
}
