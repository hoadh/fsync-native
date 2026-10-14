using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.ViewModels;
using Fsync.Core;
using Fsync.Interop.Clipboard;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Tests;

/// <summary>Review round 1 fixes against the real engine, the real clipboard and the real file system.</summary>
[Collection("clipboard")]
public sealed class Round1Tests
{
    private static ClipItem FilesClip(string itemId, params CoreFile[] files) =>
        new(itemId, new string('b', 64), "Mac mini", ClipKind.Files, 1, null, [], files, (ulong)files.Sum(f => (long)f.size), false);

    /// <summary>
    /// Eager strategy end to end on the real engine: an earlier complete download in the inbox is
    /// reused (no fetch, no "name (1)"), CF_HDROP lists the top-level folder (layout kept), the
    /// entry carries the private flags and our listener would skip it as our own echo.
    /// </summary>
    [Fact]
    public async Task EagerCopyBackReusesTheInboxAndKeepsFolders()
    {
        using var r = await RealEngine.StartAsync();
        r.Prefs.FileStrategy = FileClipStrategy.Eager;
        var item = Guid.NewGuid().ToString();
        var dir = r.Clipboard.Inbox.Folder(item);
        Directory.CreateDirectory(Path.Combine(dir, "docs", "sub"));
        File.WriteAllText(Path.Combine(dir, "docs", "sub", "a.txt"), "12345");
        File.WriteAllText(Path.Combine(dir, "top.txt"), "xy");
        var clip = FilesClip(item, new CoreFile(0, "a.txt", "docs/sub/a.txt", 5), new CoreFile(1, "top.txt", "top.txt", 2));

        Assert.True(await r.Clipboard.WriteAsync(clip), r.Clipboard.LastError);
        Assert.Equal(SkipReason.OwnEcho, ClipboardReader.Read(long.MaxValue, []).Skip);
        byte[]? drop = null;
        uint? history = null;
        Assert.True(Win32Clipboard.TryWithOpen(() =>
        {
            drop = Win32Clipboard.Read(ClipboardFormats.CF_HDROP);
            history = Win32Clipboard.ReadDword(ClipboardFormats.Register(ClipboardFormats.CanIncludeInHistory));
        }));
        Assert.Equal([Path.Combine(dir, "docs"), Path.Combine(dir, "top.txt")], DropFiles.Parse(drop!));
        Assert.Equal(0u, history);
        Assert.Equal(2, Directory.GetFiles(dir, "*", SearchOption.AllDirectories).Length); // no "(1)" copies
    }

    [Fact]
    public async Task EagerDownloadOfAnUnknownClipFailsCleanly()
    {
        using var r = await RealEngine.StartAsync();
        r.Prefs.FileStrategy = FileClipStrategy.Eager;
        Assert.False(await r.Clipboard.WriteAsync(FilesClip(Guid.NewGuid().ToString(), new CoreFile(0, "x.bin", "x.bin", 10))));
        Assert.NotNull(r.Clipboard.LastError);
    }

    [Fact]
    public void EagerOnlyUpToTheCapAndOnlyWhenChosen()
    {
        var small = FilesClip("a", new CoreFile(0, "s", "s", 1024));
        var big = FilesClip("b", new CoreFile(0, "b", "b", (ulong)EagerFiles.Cap + 1));
        Assert.True(EagerFiles.Applies(FileClipStrategy.Eager, small));
        Assert.False(EagerFiles.Applies(FileClipStrategy.Eager, big)); // lazy virtual files instead
        Assert.False(EagerFiles.Applies(FileClipStrategy.Lazy, small));
    }

    [Fact]
    public void InboxRetentionByAgeThenSize()
    {
        var root = Path.Combine(Path.GetTempPath(), "fsync-inbox-" + Guid.NewGuid().ToString("N")[..8]);
        var inbox = new InboxDirectory(root);
        try
        {
            string Make(string id, int bytes, int daysOld)
            {
                var d = inbox.Folder(id);
                File.WriteAllBytes(Path.Combine(d, "f.bin"), new byte[bytes]);
                Directory.SetLastWriteTimeUtc(d, DateTime.UtcNow.AddDays(-daysOld));
                return d;
            }
            var old = Make("aaaa", 10, 40);
            var keep = Make("bbbb", 10, 50);
            var mid = Make("cccc", 600, 3);
            var fresh = Make("dddd", 600, 1);
            Assert.Equal(2, inbox.Clean(retentionDays: 30, maxBytes: 1000, keeping: ["bbbb"]));
            Assert.False(Directory.Exists(old));   // older than the retention
            Assert.True(Directory.Exists(keep));   // the clip on the clipboard
            Assert.False(Directory.Exists(mid));   // oldest over the size limit
            Assert.True(Directory.Exists(fresh));
            Assert.Throws<ArgumentException>(() => inbox.Folder(@"..\evil"));
        }
        finally { Directory.Delete(root, true); }
    }

    [Fact]
    public async Task QuitWaitsForRunningEngineCallsWithinItsBudget()
    {
        var gate = new EngineCallGate();
        using var release = new ManualResetEventSlim();
        var call = Task.Run(() => gate.Run(() => release.Wait(5000)));
        while (gate.InFlight == 0) await Task.Delay(5);
        gate.Close();
        Assert.Throws<OperationCanceledException>(() => gate.Run(() => 1));
        Assert.False(gate.WaitIdle(TimeSpan.FromMilliseconds(100)));
        release.Set();
        Assert.True(gate.WaitIdle(TimeSpan.FromSeconds(5)));
        Assert.True(await call);
    }

    [Fact]
    public async Task SecondEngineOnTheSameFolderSaysAlreadyRunning()
    {
        using var r = await RealEngine.StartAsync();
        await r.Bridge.GoOnlineAsync(); // starts the node (takes engine.lock); no session -> signed out
        Assert.False(r.Bridge.AlreadyRunning);
        using var second = new EngineBridge(r.Settings, new SecretStore(r.Settings.SecretsPath), new InlineDispatcher());
        await second.InitializeAsync();
        await second.GoOnlineAsync();
        Assert.True(second.AlreadyRunning);
        Assert.Equal(AccountPhase.Failed, second.Phase);
        Assert.Equal(EngineBridge.AlreadyRunningMessage, second.StatusMessage);
    }

    [Fact]
    public async Task SearchWaitsForTypingToStop()
    {
        using var r = await RealEngine.StartAsync();
        var vm = new HistoryViewModel(r.Bridge, r.Clipboard);
        foreach (var q in new[] { "n", "ne", "nee", "needle" }) vm.Query = q;
        await Task.Delay(HistoryViewModel.SearchDelay * 4);
        Assert.Equal(1, vm.RefreshCount);
        Assert.Equal("No matches.", vm.Status);
    }

    [Fact]
    public async Task OffersStayUntilPulledAndBigOnesAreNeverAutoAccepted()
    {
        using var r = await RealEngine.StartAsync();
        using var vm = new TransfersViewModel(r.Bridge, () => Path.Combine(r.Dir, "dl"), () => true);
        var big = FilesClip(Guid.NewGuid().ToString(), new CoreFile(0, "big.iso", "big.iso", TransfersViewModel.AutoAcceptLimit + 1)) with { push = true };
        vm.OnIncomingFiles(big);
        Assert.Single(vm.Offers);
        Assert.Null(vm.Error); // asked, not pulled

        var small = FilesClip(Guid.NewGuid().ToString(), new CoreFile(0, "a.txt", "a.txt", 3)) with { push = true };
        var offer = new IncomingOffer(small);
        vm.Offers.Insert(0, offer);
        await vm.AcceptAsync(offer); // the core does not know this item: the pull cannot start
        Assert.NotNull(vm.Error);
        Assert.Contains(offer, vm.Offers);
    }

    [Fact]
    public void DataDirSpellingsShareOneInstanceKey()
    {
        var dir = Path.Combine(Path.GetTempPath(), "fsync-key-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(dir);
        try
        {
            AppSettings S(string d) => new(d, "http://127.0.0.1:9", null, "x");
            var key = S(dir).InstanceKey;
            Assert.Equal(key, S(dir + "\\").InstanceKey);
            Assert.Equal(key, S(dir.ToUpperInvariant()).InstanceKey);
            Assert.Equal(key, S(Path.Combine(dir, "sub", "..")).InstanceKey);
        }
        finally { Directory.Delete(dir); }
    }
}
