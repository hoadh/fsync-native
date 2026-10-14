using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>Sign-out and launch behaviour of the bridge around the pending revoke.</summary>
public sealed class PendingRevokeBridgeTests
{
    private static readonly SessionTokens Session = new("acc", "ref-1", "acct-1");
    private static PendingRevoke Pending(string refresh) => new(refresh, "https://api.example.com", DateTimeOffset.UtcNow);


    [Fact]
    public async Task SignOutKeepsThePendingRevokeBeforeLogoutAndDeletesItWhenConfirmed()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        PendingRevoke? duringLogout = null;
        h.Engine.OnLogoutConfirmed = () => { duringLogout = h.Secrets.PendingRevoke; return true; };
        await h.Bridge.LogoutAsync();
        Assert.Equal("ref-1", duringLogout?.Refresh);
        Assert.Equal("http://127.0.0.1:9", duringLogout?.ApiBase);
        Assert.Null(h.Secrets.PendingRevoke);
        Assert.Null(h.Secrets.Session);
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
    }

    [Fact]
    public async Task SignOutKeepsTheEntryWhenNotConfirmedOrFailing()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        h.Engine.OnLogoutConfirmed = () => false;
        await h.Bridge.LogoutAsync();
        Assert.Equal("ref-1", h.Secrets.PendingRevoke?.Refresh);
        Assert.Null(h.Secrets.Session);

        h.Secrets.SaveSession(new SessionTokens("a", "ref-2", "acct-1"));
        h.Engine.OnLogoutConfirmed = () => throw new FsyncException.Network("down");
        await h.Bridge.LogoutAsync();
        Assert.Equal("ref-2", h.Secrets.PendingRevoke?.Refresh);
    }

    [Fact]
    public async Task SignOutDoesNotDeleteANewerEntryThanTheOneItSaved()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        h.Engine.OnLogoutConfirmed = () => { h.Secrets.SavePendingRevoke(Pending("newer")); return true; };
        await h.Bridge.LogoutAsync();
        Assert.Equal("newer", h.Secrets.PendingRevoke?.Refresh);
    }

    [Fact]
    public async Task LaunchRetriesAPendingRevokeOffTheCallerThread()
    {
        using var h = new Harness();
        h.Secrets.SavePendingRevoke(Pending("left-over"));
        await h.Bridge.InitializeAsync();
        for (var i = 0; i < 100 && h.Secrets.PendingRevoke is not null; i++) await Task.Delay(50);
        Assert.Null(h.Secrets.PendingRevoke);
        lock (h.Revokes) Assert.Equal("left-over", Assert.Single(h.Revokes).Refresh);
    }

    [Fact]
    public async Task NonFsyncExceptionDuringLogoutStillSignsOut()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        h.Engine.OnLogoutConfirmed = () => throw new ObjectDisposedException("engine");
        await h.Bridge.LogoutAsync();
        Assert.Null(h.Secrets.Session);
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Equal("ref-1", h.Secrets.PendingRevoke?.Refresh);
    }

    [Fact]
    public async Task FailingPendingSaveStillLogsOutAndSignsOut()
    {
        using var h = new Harness(Session);
        await h.Bridge.InitializeAsync();
        Directory.CreateDirectory(Path.Combine(h.Dir, "secrets.dat.tmp")); // the atomic write now fails
        await h.Bridge.LogoutAsync();
        Assert.Contains("LogoutConfirmed", h.Engine.Calls);
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Null(new SecretStore(Path.Combine(h.Dir, "secrets.dat")).PendingRevoke);
    }

    [Fact]
    public async Task WithoutAnEngineSignOutRevokesDirectly()
    {
        using var h = new Harness(Session);
        var revokes = new List<string>();
        var settings = new AppSettings(h.Dir, "http://127.0.0.1:9", null, "pc");
        using var bridge = new EngineBridge(settings, h.Secrets, new InlineDispatcher(),
            (_, _) => throw new FsyncException.HistoryKeyMismatch(), (_, r, _) => revokes.Add(r));
        await bridge.InitializeAsync();
        await bridge.LogoutAsync();
        Assert.Equal(["ref-1"], revokes);
        Assert.Null(h.Secrets.PendingRevoke);
        Assert.Null(h.Secrets.Session);
    }

    [Fact]
    public async Task RelayListReachesTheEngineConfigAndLegacySingleUrlIsNull()
    {
        using var h = new Harness();
        EngineConfig? seen = null;
        var settings = new AppSettings(h.Dir, "http://127.0.0.1:9", "https://us.example, https://sg.example", "pc");
        using var bridge = new EngineBridge(settings, h.Secrets, new InlineDispatcher(), (cfg, _) => { seen = cfg; return h.Engine; });
        await bridge.InitializeAsync();
        Assert.Null(seen!.relayUrl);
        Assert.Equal(["https://us.example", "https://sg.example"], seen.relayUrls);
    }
}
