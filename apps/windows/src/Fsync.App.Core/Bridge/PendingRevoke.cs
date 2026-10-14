namespace Fsync.App.Bridge;

/// <summary>
/// A refresh token whose server-side revocation is not confirmed yet (sign-out, then quit).
/// Core never persists the refresh token, so <c>logout</c> may leave it valid for up to 30 days when
/// the app quits before the backend answered. The host keeps it in the DPAPI secret store until a
/// revoke is confirmed (docs/ffi-contract.md, "Hard logout after quit").
/// </summary>
public sealed record PendingRevoke(string Refresh, string ApiBase, DateTimeOffset CreatedAt, int Failures = 0)
{
    /// <summary>Give up after this many launches that could not reach the backend.</summary>
    public const int MaxFailures = 5;
    /// <summary>The refresh token expires after 30 days anyway.</summary>
    public static readonly TimeSpan MaxAge = TimeSpan.FromDays(30);

    /// <summary>Past retrying: too many failures, or old enough that the token has expired.</summary>
    public bool IsExpired(DateTimeOffset now) => Failures >= MaxFailures || now - CreatedAt >= MaxAge;

    /// <summary>Never prints the token.</summary>
    public override string ToString() => $"PendingRevoke(apiBase: {ApiBase}, failures: {Failures})";
}
