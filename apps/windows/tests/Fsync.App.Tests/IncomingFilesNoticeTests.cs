using Fsync.App.Files;
using Fsync.App.ViewModels;
using Fsync.Core;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Tests;

/// <summary>Spec section 4: notice text, suppression, coalescing, and the offer decision from TransfersViewModel.</summary>
public sealed class IncomingFilesNoticeTests
{
    private sealed class ManualTime : TimeProvider
    {
        public DateTimeOffset Now = new(2026, 10, 19, 8, 0, 0, TimeSpan.Zero);
        public override DateTimeOffset GetUtcNow() => Now;
    }

    private static ClipItem Offer(string fromName, int files = 2, ulong size = 2048, string? itemId = null) =>
        new(itemId ?? Guid.NewGuid().ToString(), new string('b', 64), fromName, ClipKind.Files, 1, null, [],
            [.. Enumerable.Range(0, files).Select(i => new CoreFile((uint)i, $"secret{i}.pdf", $"secret{i}.pdf", size / (ulong)files))], size, true);

    [Fact]
    public void TitleAndBodyFollowTheSpec()
    {
        var auto = IncomingFilesNotice.For(Offer("Mac mini"), autoAccepted: true, offerCount: 1);
        Assert.Equal("Files from Mac mini", auto.Title);
        Assert.Equal("2 files, 2 KB. Saving to Downloads", auto.Body);
        Assert.Equal("Mac mini", auto.Sender);
        Assert.Equal(1, auto.OfferCount);

        var ask = IncomingFilesNotice.For(Offer("Mac mini", files: 1, size: 10), autoAccepted: false, offerCount: 1);
        Assert.Equal("1 file, 10 B. Open fsync to accept", ask.Body);

        var many = IncomingFilesNotice.For(Offer("Mac mini"), autoAccepted: false, offerCount: 3);
        Assert.Equal("Files from Mac mini", many.Title);
        Assert.Equal("3 offers from Mac mini", many.Body);
    }

    [Fact]
    public void NeverFileNamesOrNodeIdsAndSenderIsSanitized()
    {
        var n = IncomingFilesNotice.For(Offer("‮Eve\n"), autoAccepted: false, offerCount: 1);
        Assert.Equal("Files from Eve", n.Title);
        foreach (var text in new[] { n.Title, n.Body })
        {
            Assert.DoesNotContain("secret", text);
            Assert.DoesNotContain(new string('b', 64), text);
        }
        Assert.Equal("Files from another device", IncomingFilesNotice.For(Offer("​"), false, 1).Title);
        // Built by hand, the sender is sanitized too (macOS init).
        Assert.Equal("gpj.exe", new IncomingFilesNotice("gp‮j.exe", 1, 1, false).Sender);
        Assert.Equal(new string('a', 40) + "…", new IncomingFilesNotice(new string('a', 41), 1, 1, false).Sender);
    }

    [Theory]
    [InlineData(true, true, false)]
    [InlineData(true, false, true)]
    [InlineData(false, true, true)]
    [InlineData(false, false, true)]
    public void SuppressedOnlyWhenFilesPageIsVisibleAndActive(bool active, bool filesVisible, bool show) =>
        Assert.Equal(show, IncomingFilesNotice.ShouldShow(active, filesVisible));

    // Coalescer: port of the macOS cases (explicit times), plus TimeProvider and single-held-offer body.

    private static readonly DateTimeOffset T0 = DateTimeOffset.FromUnixTimeSeconds(1000);

    private static IncomingFilesNotice N(string from = "Phone", int files = 2, ulong bytes = 2048, bool auto = false) =>
        new(from, files, bytes, auto);

    [Fact]
    public void FirstNoticePassesAndOthersInTheWindowAreFoldedIntoOne()
    {
        var c = new IncomingFilesNoticeCoalescer();
        Assert.Equal(N(), c.Submit(N(), T0));
        Assert.Null(c.Submit(N(files: 1, bytes: 10), T0.AddSeconds(2)));
        Assert.Null(c.Submit(N(files: 3, bytes: 20), T0.AddSeconds(9.9)));
        Assert.Equal(T0.AddSeconds(10), c.NextDeadline);
        Assert.Empty(c.Flush(T0.AddSeconds(9.9)));
        var outList = c.Flush(T0.AddSeconds(10));
        var merged = Assert.Single(outList);
        Assert.Equal(2, merged.OfferCount); // held offers only; the shown one is not counted
        Assert.Equal(4, merged.FileCount);
        Assert.Equal(30UL, merged.TotalBytes);
        Assert.Equal("2 offers from Phone", merged.Body);
        Assert.Null(c.NextDeadline);
    }

    [Fact]
    public void SendersAreIndependentAndWindowRestartsAfterFlushAndAfterQuiet()
    {
        var c = new IncomingFilesNoticeCoalescer();
        Assert.NotNull(c.Submit(N("A"), T0));
        Assert.NotNull(c.Submit(N("B"), T0.AddSeconds(1)));
        Assert.Null(c.Submit(N("A"), T0.AddSeconds(3)));
        Assert.Equal(["A"], c.Flush(T0.AddSeconds(10)).Select(n => n.Sender));
        // the flush opened a new window for A
        Assert.Null(c.Submit(N("A"), T0.AddSeconds(12)));
        Assert.Equal(1, c.Flush(T0.AddSeconds(20)).FirstOrDefault()?.OfferCount);
        // quiet for a whole window: next offer passes at once
        Assert.NotNull(c.Submit(N("B"), T0.AddSeconds(60)));
    }

    [Fact]
    public void LateTimerFoldsOfferIntoDueNoticeInsteadOfDroppingIt()
    {
        var c = new IncomingFilesNoticeCoalescer();
        Assert.NotNull(c.Submit(N(), T0));
        Assert.Null(c.Submit(N(files: 1, bytes: 10), T0.AddSeconds(2)));
        // the window ended at T0+10 but Flush has not run yet
        Assert.Null(c.Submit(N(files: 2, bytes: 5), T0.AddSeconds(11)));
        var due = Assert.Single(c.Flush(T0.AddSeconds(11)));
        Assert.Equal(2, due.OfferCount);
        Assert.Equal(3, due.FileCount);
        Assert.Equal(15UL, due.TotalBytes);
    }

    [Fact]
    public void OneHeldOfferRendersAsASingleOfferAndKeysAreSanitized()
    {
        var time = new ManualTime();
        var c = new IncomingFilesNoticeCoalescer(time);
        Assert.NotNull(c.Submit(IncomingFilesNotice.For(Offer("Mac"), autoAccepted: true)));
        time.Now += TimeSpan.FromSeconds(5);
        Assert.Null(c.Submit(IncomingFilesNotice.For(Offer(" Mac\n", files: 1, size: 10), autoAccepted: false))); // same sanitized sender
        Assert.Equal(time.Now.AddSeconds(5), c.NextDeadline);
        Assert.Empty(c.Flush());
        time.Now += TimeSpan.FromSeconds(5);
        var held = Assert.Single(c.Flush());
        Assert.Equal("Files from Mac", held.Title);
        Assert.Equal("1 file, 10 B. Open fsync to accept", held.Body);
        // the flush opened a new window: the next offer is held again
        Assert.Null(c.Submit(IncomingFilesNotice.For(Offer("Mac"), autoAccepted: true)));
    }

    [Theory]
    [InlineData(true, 10UL, true)]
    [InlineData(false, 10UL, false)]
    [InlineData(true, TransfersViewModel.AutoAcceptLimit + 1, false)]
    public async Task OfferReceivedCarriesTheAutoAcceptDecision(bool setting, ulong size, bool expected)
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        h.Engine.OnFetchFileTo = (_, id, _) => 100 + id;
        await h.Bridge.InitializeAsync();
        using var vm = new TransfersViewModel(h.Bridge, () => Path.Combine(h.Dir, "dl"), () => setting);
        var seen = new List<(ClipItem Offer, bool Auto)>();
        vm.OfferReceived += (o, auto) => seen.Add((o, auto));
        var offer = Offer("Mac", files: 1, size: size);
        h.Engine.Delegate!.OnIncomingFiles(offer);
        Assert.Equal([(offer, expected)], seen);
        if (expected) await BridgeTests.WaitFor(() => vm.Offers.Count == 0); // accepted
        else Assert.Single(vm.Offers);
    }

    [Fact]
    public async Task ARepeatedOfferIsAnnouncedOnce()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        await h.Bridge.InitializeAsync();
        using var vm = new TransfersViewModel(h.Bridge, () => Path.Combine(h.Dir, "dl"), () => false);
        var raised = 0;
        vm.OfferReceived += (_, _) => raised++;
        var offer = Offer("Mac", itemId: "item-1");
        h.Engine.Delegate!.OnIncomingFiles(offer);
        h.Engine.Delegate.OnIncomingFiles(offer);
        h.Engine.Delegate.OnIncomingFiles(Offer("Mac", itemId: "item-2"));
        Assert.Equal(2, raised);
    }
}
