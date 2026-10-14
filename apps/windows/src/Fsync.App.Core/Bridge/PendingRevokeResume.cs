using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Retries a pending revoke at launch with <c>FsyncFfi.RevokeSession</c> (no engine needed).</summary>
public static class PendingRevokeResume
{
    public enum Outcome
    {
        NothingPending,
        /// <summary>Revoked, or already revoked (401): deleted.</summary>
        Revoked,
        /// <summary>Backend unreachable: kept with the failure counted.</summary>
        Kept,
        /// <summary>Final refusal, too many failures or too old: deleted without a revoke.</summary>
        Dropped,
    }

    /// <summary>The real FFI call; only tests substitute it. Args: apiUrl, refreshToken, allowInsecureHttp.</summary>
    public static readonly Action<string, string, bool> DefaultRevoke = FsyncFfi.RevokeSession;

    /// <summary>Blocking (up to ~10 s): call it off the UI thread. Never logs the token.</summary>
    public static Outcome Run(SecretStore secrets, DateTimeOffset now, Action<string, string, bool> revoke)
    {
        if (secrets.PendingRevoke is not { } pending) return Outcome.NothingPending;
        if (pending.IsExpired(now))
        {
            secrets.SettlePendingRevoke(pending, null);
            return Outcome.Dropped;
        }
        try
        {
            revoke(pending.ApiBase, pending.Refresh, AppSettings.IsLoopbackHttp(pending.ApiBase));
            secrets.SettlePendingRevoke(pending, null);
            return Outcome.Revoked;
        }
        catch (FsyncException.Network)
        {
            var next = pending with { Failures = pending.Failures + 1 };
            if (next.IsExpired(now))
            {
                secrets.SettlePendingRevoke(pending, null);
                return Outcome.Dropped;
            }
            secrets.SettlePendingRevoke(pending, next);
            return Outcome.Kept;
        }
        catch (FsyncException e)
        {
            // A refusal that a retry will not change (bad URL, malformed token).
            Services.AppLog.Warn($"pending revoke dropped: {e.GetType().Name}");
            secrets.SettlePendingRevoke(pending, null);
            return Outcome.Dropped;
        }
    }
}
