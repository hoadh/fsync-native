using Fsync.App.Clipboard;

namespace Fsync.App.Tests;

/// <summary>What the tray says after a publish: a pending device is "not confirmed", not a failure.</summary>
public sealed class PublishStatusTextTests
{
    [Theory]
    [InlineData(2, 0, 0, "Sent to 2 devices", null)]
    [InlineData(1, 1, 0, "Sent to 1 device; 1 not confirmed", null)]
    [InlineData(0, 1, 0, "Not confirmed by 1 device", null)]
    [InlineData(0, 2, 1, "Not confirmed by 2 devices", "1 device(s) did not get it")]
    [InlineData(1, 0, 1, "Sent to 1 device", "1 device(s) did not get it")]
    [InlineData(0, 0, 2, "Copied, but no other device is connected", "2 device(s) did not get it")]
    [InlineData(0, 0, 0, "Copied, but no other device is connected", null)]
    public void DescribesDeliveredPendingAndFailed(int delivered, int pending, int failed, string activity, string? error)
    {
        Assert.Equal((activity, error), ClipboardService.DescribePublished(delivered, pending, failed));
    }

    [Fact]
    public void PendingIsNeverCalledFailedOrNotConnected()
    {
        foreach (var delivered in new[] { 0, 1 })
        {
            var (activity, error) = ClipboardService.DescribePublished(delivered, 1, 0);
            Assert.Null(error);
            Assert.DoesNotContain("no other device", activity);
            Assert.DoesNotContain("did not get it", activity);
        }
    }
}
