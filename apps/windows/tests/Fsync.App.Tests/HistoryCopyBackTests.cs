using System.Text;
using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.App.Services;
using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Tests;

/// <summary>History "Copy" with the full content from core (history_item_reps), over the scripted engine seam.</summary>
[Collection("clipboard")]
public sealed class HistoryCopyBackTests : IDisposable
{
    private readonly Harness _h = new();
    private readonly ClipboardService _svc;

    public HistoryCopyBackTests()
    {
        _svc = new ClipboardService(_h.Bridge, AppPreferences.Load(Path.Combine(_h.Dir, "prefs.json")), new InlineDispatcher(), Path.Combine(_h.Dir, "inbox"));
    }

    public void Dispose()
    {
        _svc.Dispose();
        _h.Bridge.Dispose();
        _h.Dispose();
    }

    private static HistoryItem Item(ClipKind kind = ClipKind.Text, bool outgoing = false) =>
        new(Guid.NewGuid().ToString(), outgoing, null, kind, 1, 4, "x", false);

    [Fact]
    public async Task CopiesAnOlderItemWithItsFullContent()
    {
        await _h.Bridge.InitializeAsync();
        var item = Item();
        _h.Engine.OnHistoryItemReps = _ => [new Rep("text/plain", Encoding.UTF8.GetBytes("older clip")), new Rep("text/html", Encoding.UTF8.GetBytes("<b>older clip</b>"))];
        ClipDataObject? written = null;
        _svc.Written += o => written = o;
        Assert.True(await _svc.CopyBackAsync(item));
        Assert.Contains($"HistoryItemReps {item.id}", _h.Engine.Calls);
        Assert.NotNull(written);
        Assert.False(written!.HasVirtualFiles);
    }

    [Fact]
    public async Task FailsWhenCoreHasNoContentOrTheItemIsFilesOnly()
    {
        await _h.Bridge.InitializeAsync();
        Assert.False(await _svc.CopyBackAsync(Item())); // core: NotFound
        Assert.False(_svc.CanCopyBack(Item(ClipKind.Files)));
        Assert.False(await _svc.CopyBackAsync(Item(ClipKind.Files)));
        lock (_h.Engine.Calls) Assert.Equal(1, _h.Engine.Calls.Count(c => c.StartsWith("HistoryItemReps"))); // the files item never asked core
        _h.Engine.OnHistoryItemReps = _ => [new Rep("application/x-unknown", [1, 2, 3])];
        Assert.False(await _svc.CopyBackAsync(Item())); // nothing this PC can paste
    }

    [Fact]
    public async Task ANewerWriteStopsWaitingForASlowHistoryCopy()
    {
        await _h.Bridge.InitializeAsync();
        using var release = new ManualResetEventSlim();
        _h.Engine.OnHistoryItemReps = _ => { release.Wait(TimeSpan.FromSeconds(30)); return [new Rep("text/plain", [65])]; };
        var slow = _svc.CopyBackAsync(Item());
        for (var i = 0; i < 200 && !_h.Engine.Calls.Exists(c => c.StartsWith("HistoryItemReps")); i++) await Task.Delay(25);
        var newer = new ClipItem(Guid.NewGuid().ToString(), new string('b', 64), "Mac", ClipKind.Text, 1, "hi", [new RepInfo("text/plain", 2)], [], 2, false);
        _h.Engine.OnHistoryItemReps = _ => [];
        var newerWrite = _svc.WriteAsync(newer); // supersedes: cancels the slow copy's ticket
        try { Assert.False(await slow.WaitAsync(TimeSpan.FromSeconds(10))); } // not held until core answers
        finally { release.Set(); }
        await newerWrite.WaitAsync(TimeSpan.FromSeconds(10));
    }

    [Fact]
    public async Task AnEmptyRepListSaysSoAndABigPngIsKeptLazy()
    {
        await _h.Bridge.InitializeAsync();
        _h.Engine.OnHistoryItemReps = _ => [];
        Assert.False(await _svc.CopyBackAsync(Item()));
        Assert.Equal("This item has no content this PC can paste", _svc.LastError);
        var big = new byte[(int)IncomingClip.EagerRepBytes + 1];
        // Garbage "PNG": a big one is not decoded until a target reads it (2 formats); a small one fails the DIB conversion now and keeps PNG only.
        Assert.Equal(2, ClipboardService.EntriesFor([new Rep("image/png", big)]).Count);
        Assert.Single(ClipboardService.EntriesFor([new Rep("image/png", [1, 2, 3])]));
    }

    [Fact]
    public void FirstRepOfAFormatWinsAndUnknownMimeTypesAreDropped()
    {
        var entries = ClipboardService.EntriesFor([
            new Rep("text/plain", Encoding.UTF8.GetBytes("a")), new Rep("text/plain", Encoding.UTF8.GetBytes("b")), new Rep("application/x-unknown", [1])]);
        var only = Assert.Single(entries);
        Assert.Equal(ClipboardFormats.CF_UNICODETEXT, only.Format);
    }
}
