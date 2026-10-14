using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Tests;

public sealed class BridgeTests
{
    private static readonly SessionTokens Session = new("acc", "ref", "acct-1");

    [Fact]
    public async Task NoSessionMeansSignedOutWithoutStartingTheNode()
    {
        using var h = new Harness();
        await h.Bridge.InitializeAsync();
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.DoesNotContain("Start", h.Engine.Calls);
    }

    [Theory]
    [InlineData(AccountState.Ready, AccountPhase.Ready)]
    [InlineData(AccountState.Created, AccountPhase.Ready)]
    [InlineData(AccountState.NeedsPairing, AccountPhase.NeedsPairing)]
    public async Task StoredSessionIsRestoredAndMapped(AccountState state, AccountPhase phase)
    {
        using var h = new Harness(Session);
        h.Engine.OnEnsure = () => state;
        await h.Bridge.InitializeAsync();
        Assert.Equal(phase, h.Bridge.Phase);
        Assert.Equal(["SetSession acct-1", "Start", "EnsureAccount"], h.Engine.Calls.Take(3));
    }

    [Fact]
    public async Task AuthFailureClearsTheSession()
    {
        using var h = new Harness(Session);
        h.Engine.OnEnsure = () => throw new FsyncException.Auth();
        await h.Bridge.InitializeAsync();
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Null(h.Secrets.Session);
    }

    [Fact]
    public async Task NetworkFailureKeepsTheSessionAndAllowsRetry()
    {
        using var h = new Harness(Session);
        h.Engine.OnEnsure = () => throw new FsyncException.Network("connection refused");
        await h.Bridge.InitializeAsync();
        Assert.Equal(AccountPhase.Offline, h.Bridge.Phase);
        Assert.Contains("connection refused", h.Bridge.StatusMessage);
        Assert.NotNull(h.Secrets.Session);

        h.Engine.OnEnsure = () => AccountState.Ready;
        await h.Bridge.GoOnlineAsync();
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        Assert.Null(h.Bridge.StatusMessage);
    }

    [Fact]
    public async Task HistoryKeyMismatchIsReportedNotHidden()
    {
        using var h = new Harness();
        var settings = new AppSettings(h.Dir, "http://127.0.0.1:9", null, "pc");
        using var bridge = new EngineBridge(settings, h.Secrets, new InlineDispatcher(),
            (_, _) => throw new FsyncException.HistoryKeyMismatch());
        await bridge.InitializeAsync();
        Assert.Equal(AccountPhase.Failed, bridge.Phase);
        Assert.True(bridge.HistoryKeyMismatch);
    }

    [Fact]
    public async Task LoginVerifyStoresTokensAndEmail()
    {
        using var h = new Harness();
        await h.Bridge.InitializeAsync();
        h.Engine.OnEnsure = () => AccountState.Created;
        await h.Bridge.LoginVerifyAsync("me@example.com", "123456");
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        Assert.Equal("acct", h.Secrets.Session?.accountId);
        Assert.Equal("me@example.com", h.Secrets.Email);
    }

    [Fact]
    public async Task CoreCallbacksPersistTokensAndHandleAccountEvents()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        h.Engine.Delegate!.OnSessionChanged(new SessionTokens("new-a", "new-r", "acct-1"));
        Assert.Equal("new-a", new SecretStore(Path.Combine(h.Dir, "secrets.dat")).Session?.access);

        h.Engine.Delegate.OnAccountEvent(new AccountEvent.SelfRemoved());
        Assert.Equal(AccountPhase.NeedsPairing, h.Bridge.Phase);

        h.Engine.Delegate.OnAccountEvent(new AccountEvent.SessionExpired());
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Null(h.Secrets.Session);
    }

    [Fact]
    public async Task CompletedPairingRefreshesTheAccountState()
    {
        using var h = new Harness(Session);
        h.Engine.OnEnsure = () => AccountState.NeedsPairing;
        await h.Bridge.InitializeAsync();
        h.Engine.OnEnsure = () => AccountState.Ready;
        h.Engine.Delegate!.OnPairingEvent(new PairingEvent.Completed(new string('b', 64), 2));
        await WaitFor(() => h.Bridge.Phase == AccountPhase.Ready);
    }

    internal static async Task WaitFor(Func<bool> cond)
    {
        for (var i = 0; i < 200 && !cond(); i++) await Task.Delay(10);
        Assert.True(cond());
    }
}
