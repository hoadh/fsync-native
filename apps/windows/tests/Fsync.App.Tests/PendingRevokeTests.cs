using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Tests;

public sealed class PendingRevokeTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 15, 0, 0, 0, TimeSpan.Zero);
    private static readonly SessionTokens Session = new("acc", "ref-1", "acct-1");
    private static PendingRevoke Pending(string refresh = "ref-1", int failures = 0, DateTimeOffset? at = null) =>
        new(refresh, "https://api.example.com", at ?? Now, failures);

    // ---- relay list (S6)

    [Theory]
    [InlineData(null, new string[0])]
    [InlineData("", new string[0])]
    [InlineData("https://r1.example", new[] { "https://r1.example" })]
    [InlineData(" https://us.example , https://sg.example ,, ", new[] { "https://us.example", "https://sg.example" })]
    public void RelayListIsSplitTrimmedAndNonEmpty(string? raw, string[] expected) =>
        Assert.Equal(expected, AppSettings.SplitRelays(raw));

    [Fact]
    public void RelayEnvVarAcceptsOneUrlOrAList()
    {
        string? Env(string k) => k switch { "FSYNC_RELAY" => "https://a.example,https://b.example", "FSYNC_DATA_DIR" => Path.GetTempPath(), _ => null };
        Assert.Equal(["https://a.example", "https://b.example"], AppSettings.FromEnvironment(Env).RelayUrls);
        Assert.Empty(AppSettings.FromEnvironment(_ => null).RelayUrls);
    }

    // ---- store

    private static SecretStore NewStore(out string path)
    {
        path = Path.Combine(Path.GetTempPath(), "fsync-pr-" + Guid.NewGuid().ToString("N") + ".dat");
        return new SecretStore(path);
    }

    [Fact]
    public void PendingRevokeSurvivesReopenAndIsTokenMatchedOnSettle()
    {
        var store = NewStore(out var path);
        try
        {
            store.SavePendingRevoke(Pending("old"));
            Assert.Equal("old", new SecretStore(path).PendingRevoke?.Refresh);

            store.SavePendingRevoke(Pending("new")); // newer sign-out replaces it
            store.SettlePendingRevoke(Pending("old"), null); // retry of the old token finishes late
            Assert.Equal("new", store.PendingRevoke?.Refresh);

            store.SettlePendingRevoke(Pending("new"), Pending("new", failures: 2));
            Assert.Equal(2, store.PendingRevoke?.Failures);
            store.SettlePendingRevoke(Pending("new"), null);
            Assert.Null(new SecretStore(path).PendingRevoke);
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void PendingRevokeNeverPrintsTheToken() => Assert.DoesNotContain("secret-token", Pending("secret-token").ToString());

    [Fact]
    public void ExpiryByFailuresOrAge()
    {
        Assert.False(Pending(failures: 4).IsExpired(Now));
        Assert.True(Pending(failures: 5).IsExpired(Now));
        Assert.False(Pending().IsExpired(Now + TimeSpan.FromDays(29)));
        Assert.True(Pending().IsExpired(Now + TimeSpan.FromDays(30)));
    }

    // ---- resume

    [Fact]
    public void ResumeRevokesAndDeletes()
    {
        var store = NewStore(out var path);
        try
        {
            store.SavePendingRevoke(Pending());
            (string, string, bool)? seen = null;
            Assert.Equal(PendingRevokeResume.Outcome.Revoked, PendingRevokeResume.Run(store, Now, (a, r, i) => seen = (a, r, i)));
            Assert.Equal(("https://api.example.com", "ref-1", false), seen);
            Assert.Null(store.PendingRevoke);
            Assert.Equal(PendingRevokeResume.Outcome.NothingPending, PendingRevokeResume.Run(store, Now, (_, _, _) => throw new InvalidOperationException()));
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void ResumeKeepsOnNetworkFailureThenDropsAfterFiveFailures()
    {
        var store = NewStore(out var path);
        try
        {
            store.SavePendingRevoke(Pending());
            void Fail(string a, string r, bool i) => throw new FsyncException.Network("down");
            for (var n = 1; n <= 4; n++)
            {
                Assert.Equal(PendingRevokeResume.Outcome.Kept, PendingRevokeResume.Run(store, Now, Fail));
                Assert.Equal(n, store.PendingRevoke?.Failures);
            }
            Assert.Equal(PendingRevokeResume.Outcome.Dropped, PendingRevokeResume.Run(store, Now, Fail));
            Assert.Null(store.PendingRevoke);
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void ResumeDropsOldEntriesWithoutCallingTheBackendAndRefusalsWithoutRetry()
    {
        var store = NewStore(out var path);
        try
        {
            store.SavePendingRevoke(Pending(at: Now - TimeSpan.FromDays(31)));
            Assert.Equal(PendingRevokeResume.Outcome.Dropped, PendingRevokeResume.Run(store, Now, (_, _, _) => throw new InvalidOperationException("must not call")));
            Assert.Null(store.PendingRevoke);

            store.SavePendingRevoke(Pending());
            Assert.Equal(PendingRevokeResume.Outcome.Dropped, PendingRevokeResume.Run(store, Now, (_, _, _) => throw new FsyncException.Invalid("bad url")));
            Assert.Null(store.PendingRevoke);
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void NewerTokenSavedDuringRetryIsKept()
    {
        var store = NewStore(out var path);
        try
        {
            store.SavePendingRevoke(Pending("old"));
            // A new sign-out stores its token while the retry of the old one is on the wire.
            var outcome = PendingRevokeResume.Run(store, Now, (_, _, _) => store.SavePendingRevoke(Pending("new")));
            Assert.Equal(PendingRevokeResume.Outcome.Revoked, outcome);
            Assert.Equal("new", store.PendingRevoke?.Refresh);

            // Same on a failed retry: the failure count must not overwrite the newer entry.
            PendingRevokeResume.Run(store, Now, (_, _, _) => { store.SavePendingRevoke(Pending("newest")); throw new FsyncException.Network("x"); });
            Assert.Equal(("newest", 0), (store.PendingRevoke?.Refresh, store.PendingRevoke?.Failures));
        }
        finally { File.Delete(path); }
    }
}
