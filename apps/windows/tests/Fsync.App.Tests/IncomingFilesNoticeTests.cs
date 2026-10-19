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
        Assert.False(auto.IsUpdate);

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
    }

    [Theory]
    [InlineData(true, true, false)]
    [InlineData(true, false, true)]
    [InlineData(false, true, true)]
    [InlineData(false, false, true)]
    public void SuppressedOnlyWhenFilesPageIsVisibleAndActive(bool active, bool filesVisible, bool show) =>
        Assert.Equal(show, IncomingFilesNotice.ShouldShow(active, filesVisible));

    [Fact]
    public void CoalescesPerSenderForTenSeconds()
    {
        var time = new ManualTime();
        var c = new IncomingFilesNoticeCoalescer(time);
        var first = c.Add(Offer("Mac"), autoAccepted: true);
        Assert.False(first.IsUpdate);
        Assert.Equal(1, first.Count);

        time.Now += TimeSpan.FromSeconds(4);
        var other = c.Add(Offer("Phone"), autoAccepted: false); // another sender: its own notice
        Assert.False(other.IsUpdate);

        time.Now += TimeSpan.FromSeconds(5);
        var second = c.Add(Offer(" Mac\n"), autoAccepted: false); // same sanitized sender
        Assert.True(second.IsUpdate);
        Assert.Equal(2, second.Count);
        Assert.Equal("2 offers from Mac", second.Body);

        time.Now += TimeSpan.FromSeconds(1); // 10 s after the first notice: a new window
        var third = c.Add(Offer("Mac"), autoAccepted: true);
        Assert.False(third.IsUpdate);
        Assert.Equal(1, third.Count);
        Assert.Equal("2 files, 2 KB. Saving to Downloads", third.Body);
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
