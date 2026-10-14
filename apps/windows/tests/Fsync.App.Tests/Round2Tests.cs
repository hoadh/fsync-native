using Fsync.App.Clipboard;
using Fsync.Core;
using Fsync.Interop.Clipboard;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Tests;

/// <summary>Review round 2: in-flight downloads vs cleanup, the Done-event race, eager timeout and cancel, supersede.</summary>
[Collection("clipboard")]
public sealed class Round2Tests
{
    private static TransferEvent Done(ulong id, string path) =>
        new(id, "item", 0, TransferDirection.Receive, TransferState.Done, 1, 1, path, null);

    [Fact]
    public async Task CleanupLeavesFoldersADownloadIsWritingInto()
    {
        using var r = await RealEngine.StartAsync();
        await r.Clipboard.RefreshSettingsAsync();
        var days = (int)r.Clipboard.CurrentSettings!.retentionDays;
        var id = Guid.NewGuid().ToString();
        var dir = r.Clipboard.Inbox.Folder(id);
        File.WriteAllText(Path.Combine(dir, "part.bin"), "partial");
        void Age() => Directory.SetLastWriteTimeUtc(dir, DateTime.UtcNow.AddDays(-days - 10));

        using (r.Clipboard.BeginInFlight(id))
        {
            Age();
            Assert.Equal(0, r.Clipboard.CleanInbox());
            Assert.True(Directory.Exists(dir));
        }
        Age();
        Assert.Equal(1, r.Clipboard.CleanInbox()); // released: old enough to go
        Assert.False(Directory.Exists(dir));

        var fresh = r.Clipboard.Inbox.Folder(Guid.NewGuid().ToString()); // just created: younger than MinAge
        Assert.Equal(0, r.Clipboard.Inbox.Clean(0, 0, []));
        Assert.True(Directory.Exists(fresh));
    }

    [Fact]
    public async Task DoneEventIsNeverLostWhicheverSideComesFirst()
    {
        using var r = await RealEngine.StartAsync();
        var svc = r.Clipboard;
        svc.OnTransfer(Done(1, "early"));                 // outcome before registration
        Assert.Equal("early", await svc.Register(1).WaitAsync(TimeSpan.FromSeconds(5)));
        var late = svc.Register(2);                        // registration before the outcome
        svc.OnTransfer(Done(2, "late"));
        Assert.Equal("late", await late.WaitAsync(TimeSpan.FromSeconds(5)));

        // Both orders racing on different threads: every registration completes.
        var ids = Enumerable.Range(100, 2000).Select(i => (ulong)i).ToArray();
        var registered = new Task<string>[ids.Length];
        var events = Task.Run(() => { foreach (var i in ids) svc.OnTransfer(Done(i, $"p{i}")); });
        var regs = Task.Run(() => { for (var k = 0; k < ids.Length; k++) registered[k] = svc.Register(ids[k]); });
        await Task.WhenAll(events, regs);
        await Task.WhenAll(registered).WaitAsync(TimeSpan.FromSeconds(10));
        Assert.All(registered.Select((t, k) => (t.Result, k)), x => Assert.Equal($"p{ids[x.k]}", x.Result));
    }

    [Fact]
    public async Task EagerWaitGivesUpAfterItsTimeoutAndCancelsTheTransfer()
    {
        using var r = await RealEngine.StartAsync();
        r.Clipboard.EagerTimeout = TimeSpan.FromMilliseconds(200);
        await Assert.ThrowsAsync<TimeoutException>(() => r.Clipboard.AwaitDownloadsAsync([424_242], CancellationToken.None));
        Assert.Contains(424_242UL, r.Clipboard.CancelledTransfers);
    }

    [Fact]
    public async Task ANewerClipCancelsTheOlderClipsDownloads()
    {
        using var r = await RealEngine.StartAsync();
        var older = r.Clipboard.AwaitDownloadsAsync([515_151], r.Clipboard.LatestWriteToken); // the 180 s wait
        var text = new ClipItem(Guid.NewGuid().ToString(), new string('b', 64), "Mac", ClipKind.Text, 1, "hi",
            [new RepInfo("text/plain", 2)], [], 2, false);
        await r.Clipboard.WriteAsync(text); // fails to fetch (unknown item), but it is newer
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => older.WaitAsync(TimeSpan.FromSeconds(5)));
        Assert.Contains(515_151UL, r.Clipboard.CancelledTransfers);
    }

    [Fact]
    public async Task AnOlderLocalChangeIsNotPublishedAfterANewerOne()
    {
        using var r = await RealEngine.StartAsync();
        var snap = new ClipboardSnapshot { Formats = [], Text = "older" };
        var older = r.Clipboard.NextOutGeneration();
        var newer = r.Clipboard.NextOutGeneration();
        Assert.Equal(ClipboardService.PublishOutcome.Superseded, await r.Clipboard.PublishAsync(snap, older));
        Assert.NotEqual(ClipboardService.PublishOutcome.Superseded, await r.Clipboard.PublishAsync(snap, newer));
    }

    [Fact]
    public async Task APendingClipStaysPendingWhenItCannotBeWritten()
    {
        using var r = await RealEngine.StartAsync();
        var clip = new ClipItem(Guid.NewGuid().ToString(), new string('b', 64), "Mac", ClipKind.Files, 1, null, [],
            [new CoreFile(0, "x.bin", "x.bin", 10)], 10, false);
        r.Prefs.FileStrategy = FileClipStrategy.Eager;
        r.Clipboard.Pending = clip;
        Assert.False(await r.Clipboard.PastePendingAsync());
        Assert.Same(clip, r.Clipboard.Pending);
        Assert.Empty(EagerFiles.TopLevel(r.Dir, [new CoreFile(0, "con", "CON", 1)])); // nothing usable: no empty CF_HDROP
    }
}
