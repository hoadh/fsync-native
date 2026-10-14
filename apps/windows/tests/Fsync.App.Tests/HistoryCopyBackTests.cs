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
    public void FirstRepOfAFormatWinsAndUnknownMimeTypesAreDropped()
    {
        var entries = ClipboardService.EntriesFor([
            new Rep("text/plain", Encoding.UTF8.GetBytes("a")), new Rep("text/plain", Encoding.UTF8.GetBytes("b")), new Rep("application/x-unknown", [1])]);
        var only = Assert.Single(entries);
        Assert.Equal(ClipboardFormats.CF_UNICODETEXT, only.Format);
    }
}
