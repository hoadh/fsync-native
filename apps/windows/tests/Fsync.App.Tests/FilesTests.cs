using Fsync.App.Files;
using Fsync.Core;

namespace Fsync.App.Tests;

public sealed class FilesTests
{
    [Fact]
    public void SendToArgumentsKeepOnlyExistingAbsolutePaths()
    {
        var dir = Path.Combine(Path.GetTempPath(), "fsync-sendto-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(dir);
        try
        {
            var a = Path.Combine(dir, "a b.txt");
            File.WriteAllText(a, "x");
            Assert.Null(SendToArgs.Parse(["--background"]));
            Assert.Equal([a, dir], SendToArgs.Parse(["--send", a, "relative.txt", Path.Combine(dir, "missing"), dir, a]));
            Assert.Empty(SendToArgs.Parse(["--send"])!);
        }
        finally { Directory.Delete(dir, true); }
    }

    [Fact]
    public void RedirectedCommandLinesSplitLikeTheCRuntime()
    {
        Assert.Equal(["--send", @"C:\a b\c.txt", @"D:\x"], SendToArgs.Split(@"--send ""C:\a b\c.txt"" D:\x"));
        Assert.Empty(SendToArgs.Split("  "));
    }

    [Fact]
    public void TransferRowsFollowEvents()
    {
        var t = new TransferItem(7, "report.pdf to Mac", TransferDirection.Send);
        t.Apply(new TransferEvent(7, "i", 0, TransferDirection.Send, TransferState.Progress, 512 * 1024, 2 * 1024 * 1024, null, null));
        Assert.True(t.IsActive);
        Assert.Equal(25, t.Percent);
        Assert.Equal("512 KB of 2 MB", t.Detail);
        t.Apply(new TransferEvent(7, "i", 0, TransferDirection.Send, TransferState.Failed, 0, 0, null, "peer went away"));
        Assert.False(t.IsActive);
        Assert.Equal("Failed: peer went away", t.Detail);
        Assert.Equal("report.pdf to Mac, Failed: peer went away", t.ToString());
    }

    [Fact]
    public void OffersDescribeWhatArrives()
    {
        Core.FileInfo F(uint id, string n) => new(id, n, n, 1000);
        ClipItem Offer(params Core.FileInfo[] files) => new("i", "n", "Mac mini", ClipKind.Files, 1, null, [], files, 3 * 1024 * 1024, true);
        Assert.Equal("Mac mini sent a.pdf (3 MB)", new IncomingOffer(Offer(F(0, "a.pdf"))).Title);
        Assert.Equal("Mac mini sent 2 files (3 MB)", new IncomingOffer(Offer(F(0, "a"), F(1, "b"))).Title);
    }
}
