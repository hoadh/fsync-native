using Fsync.App.Clipboard;
using Fsync.App.Services;
using Fsync.Core;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Tests;

/// <summary>
/// An eager download must cancel every transfer core already started when it stops (newer clip, quit,
/// budget, a sibling failing), including one whose start returns only after we stopped waiting. The peer
/// side cannot run in a unit test, so the engine seam (IFsyncEngine) is scripted: these tests pin OUR
/// bookkeeping (which ids get cancelled, when); core's own cancel is covered by the engine's tests.
/// </summary>
[Collection("clipboard")]
public sealed class CancelStartedTransferTests : IDisposable
{
    private readonly Harness _h = new();
    private readonly ClipboardService _svc;

    public CancelStartedTransferTests()
    {
        _svc = new ClipboardService(_h.Bridge, AppPreferences.Load(Path.Combine(_h.Dir, "prefs.json")), new InlineDispatcher(), Path.Combine(_h.Dir, "inbox"));
    }

    public void Dispose()
    {
        _svc.Dispose();
        _h.Bridge.Dispose();
        _h.Dispose();
    }

    private static ClipItem Clip(int files) =>
        new(Guid.NewGuid().ToString(), new string('b', 64), "Mac", ClipKind.Files, 1, null, [],
            [.. Enumerable.Range(0, files).Select(i => new CoreFile((uint)i, $"f{i}.bin", $"f{i}.bin", 10))], 10 * (ulong)files, false);

    private List<ulong> Cancelled() { lock (_h.Engine.Cancelled) return [.. _h.Engine.Cancelled]; }

    private async Task<List<ulong>> CancelledEventually(int count)
    {
        for (var i = 0; i < 200 && Cancelled().Count < count; i++) await Task.Delay(25);
        return Cancelled();
    }

    [Fact]
    public async Task CallerCancelAfterTheTransferStartedCancelsIt()
    {
        await _h.Bridge.InitializeAsync();
        _h.Engine.OnFetchFileTo = (_, _, _) => 41;
        using var cts = new CancellationTokenSource();
        var download = _svc.DownloadAsync(Clip(1), cts.Token); // started, never finishes: no Done event
        for (var i = 0; i < 200 && !_h.Engine.Calls.Contains("FetchFileTo 0"); i++) await Task.Delay(25);
        cts.Cancel(); // a newer clip, or quit
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => download);
        Assert.Equal([41UL], await CancelledEventually(1));
        Assert.Equal([41UL], _svc.CancelledTransfers.ToArray());
    }

    [Fact]
    public async Task BudgetRunningOutCancelsTheStartedTransferAndReportsATimeout()
    {
        await _h.Bridge.InitializeAsync();
        _h.Engine.OnFetchFileTo = (_, _, _) => 42;
        _svc.EagerTimeout = TimeSpan.FromMilliseconds(300);
        await Assert.ThrowsAsync<TimeoutException>(() => _svc.DownloadAsync(Clip(1), CancellationToken.None));
        Assert.Equal([42UL], await CancelledEventually(1));
    }

    [Fact]
    public async Task AStartThatReturnsAfterWeStoppedWaitingIsCancelledWhenItDoes()
    {
        await _h.Bridge.InitializeAsync();
        using var release = new ManualResetEventSlim();
        _h.Engine.OnFetchFileTo = (_, _, _) => { release.Wait(TimeSpan.FromSeconds(30)); return 43; }; // the dial
        _svc.EagerTimeout = TimeSpan.FromMilliseconds(200);
        await Assert.ThrowsAsync<TimeoutException>(() => _svc.DownloadAsync(Clip(1), CancellationToken.None));
        Assert.Empty(Cancelled()); // nothing started yet, nothing to cancel yet
        release.Set(); // core finally returns the id
        Assert.Equal([43UL], await CancelledEventually(1));
    }

    [Fact]
    public async Task AFailingSecondStartCancelsTheFirstTransfer()
    {
        await _h.Bridge.InitializeAsync();
        _h.Engine.OnFetchFileTo = (_, fileId, _) => fileId == 0 ? 44 : throw new FsyncException.Network("down");
        await Assert.ThrowsAnyAsync<FsyncException>(() => _svc.DownloadAsync(Clip(2), CancellationToken.None));
        Assert.Equal([44UL], await CancelledEventually(1));
    }

    [Fact]
    public async Task AFinishedTransferIsNotCancelled()
    {
        await _h.Bridge.InitializeAsync();
        var item = Clip(1);
        _h.Engine.OnFetchFileTo = (_, _, dir) =>
        {
            File.WriteAllText(Path.Combine(dir, "f0.bin"), "0123456789");
            _svc.OnTransfer(new TransferEvent(45, item.itemId, 0, TransferDirection.Receive, TransferState.Done, 10, 10, Path.Combine(dir, "f0.bin"), null));
            return 45;
        };
        var tops = await _svc.DownloadAsync(item, CancellationToken.None);
        Assert.Single(tops);
        Assert.Empty(Cancelled());
    }
}
