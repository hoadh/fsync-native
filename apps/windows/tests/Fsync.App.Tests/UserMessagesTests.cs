using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Tests;

public sealed class UserMessagesTests
{
    [Fact]
    public void PeerRefusedSaysTheDeviceRemovedThisPc()
    {
        Assert.Equal("That device removed this PC. Pair again to send to it.",
            UserMessages.For(new FsyncException.PeerRefused()));
    }

    [Fact]
    public void PeerRefusedIsNotTheLocalNotPairedText()
    {
        Assert.NotEqual(UserMessages.For(new FsyncException.NotPaired()),
            UserMessages.For(new FsyncException.PeerRefused()));
    }
}
