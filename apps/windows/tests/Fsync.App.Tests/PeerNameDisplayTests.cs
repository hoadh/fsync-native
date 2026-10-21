using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.ViewModels;
using Fsync.Core;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Tests;

/// <summary>
/// Peer-supplied names in visible text go through PeerText.DisplayName (spec section 5): control and
/// format characters (newline, bidi override, zero-width) are dropped, and an empty result reads
/// "another device". Covers the history row, the offer title and the tray tooltip activity strings.
/// </summary>
public sealed class PeerNameDisplayTests
{
    public static TheoryData<string, string> Names => new()
    {
        { "Mac\nmini", "Macmini" },
        { "\U0000202Eevil", "evil" },
        { "a\U0000200Db\U0000200Bc", "abc" },
        { "", "another device" },
        { "\U00000007\U0000200B\n", "another device" },
    };

    private static HistoryRow Row(string peerName) =>
        new(new HistoryItem("id", false, "peer-node", ClipKind.Text, 0, 10, "x", false), peerName, true);

    private static ClipItem Clip(string fromName, ClipKind kind = ClipKind.Text, int files = 0) =>
        new(Guid.NewGuid().ToString(), new string('b', 64), fromName, kind, 1, null, [],
            [.. Enumerable.Range(0, files).Select(i => new CoreFile((uint)i, $"f{i}.txt", $"f{i}.txt", 1))], 1, false);

    [Theory]
    [MemberData(nameof(Names))]
    public void HistoryRowMetaShowsTheSanitizedPeer(string raw, string shown)
    {
        var meta = Row(raw).Meta;
        Assert.StartsWith($"From {shown} · ", meta); // the time after it is culture-formatted
        AssertSeparatorsSpaced(meta);
    }

    [Fact]
    public void HistoryRowMetaForThisPcHasNoPeer()
    {
        var mine = new HistoryRow(new HistoryItem("id", true, null, ClipKind.Text, 0, 10, "x", false), "\U0000202Eevil", true);
        Assert.StartsWith("Copied here · ", mine.Meta);
        Assert.DoesNotContain("evil", mine.Meta);
        AssertSeparatorsSpaced(mine.Meta);
    }

    /// <summary>Every middle dot is followed by a space ("From Mac · 14:03 · 1 B", not "·14:03").</summary>
    private static void AssertSeparatorsSpaced(string meta) =>
        Assert.DoesNotMatch("·[^ ]", meta);

    [Theory]
    [MemberData(nameof(Names))]
    public void ClipboardActivityShowsTheSanitizedSender(string raw, string shown)
    {
        Assert.Equal($"Text from {shown}", ClipboardService.ReceivedActivity(Clip(raw)));
        Assert.Equal($"Text from {shown}: press {ClipboardService.PasteHotKeyText} to paste", ClipboardService.PendingActivity(Clip(raw)));
        Assert.Equal($"Downloading 2 file(s) from {shown}", ClipboardService.DownloadingActivity(2, Clip(raw, ClipKind.Files, 2)));
    }

    [Theory]
    [MemberData(nameof(Names))]
    public void OfferTitleShowsTheSanitizedSender(string raw, string shown)
    {
        Assert.Equal($"{shown} sent 2 files (1 B)", new IncomingOffer(Clip(raw, ClipKind.Files, 2)).Title);
        Assert.StartsWith($"{shown} sent f0.txt", new IncomingOffer(Clip(raw, ClipKind.Files, 1)).Title);
    }

    /// <summary>The single file name is peer-supplied too: a bidi override cannot flip "gpj.exe" into "exe.jpg".</summary>
    [Fact]
    public void OfferTitleSanitizesTheFileName()
    {
        var clip = Clip("Mac", ClipKind.Files, 0) with
        {
            files = [new CoreFile(0, "gp\U0000202Ej.exe", "gp\U0000202Ej.exe", 1)],
        };
        Assert.Equal("Mac sent gpj.exe (1 B)", new IncomingOffer(clip).Title);
    }
}
