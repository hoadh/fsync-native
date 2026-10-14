namespace Fsync.App.Bridge;

/// <summary>
/// The pending revoke lives in the same DPAPI file as the session. One lock covers save and
/// compare-and-settle: a retry in flight must not touch an entry that a newer sign-out replaced.
/// The lock is never held across a network call.
/// </summary>
public sealed partial class SecretStore
{
    public PendingRevoke? PendingRevoke { get { lock (_lock) return _data.PendingRevoke; } }

    /// <summary>Persists (replaces) the pending revoke. Call BEFORE <c>LogoutConfirmed</c>.</summary>
    public void SavePendingRevoke(PendingRevoke pending)
    {
        lock (_lock)
        {
            _data.PendingRevoke = pending;
            Save();
        }
    }

    /// <summary>
    /// Reads the newest session and saves its refresh token as the pending revoke under ONE lock, so a
    /// rotation arriving on the callback thread cannot slip in between. Null when signed out.
    /// </summary>
    public PendingRevoke? SavePendingRevokeFromSession(string apiBase, DateTimeOffset now)
    {
        lock (_lock)
        {
            if (_data.Refresh is not { } refresh) return null;
            var pending = new PendingRevoke(refresh, apiBase, now);
            _data.PendingRevoke = pending;
            Save();
            return pending;
        }
    }

    /// <summary>
    /// After a retry of <paramref name="retried"/>: deletes the entry (<paramref name="replacement"/> null) or
    /// replaces it, but only while it still holds the same refresh token. A newer token is kept untouched.
    /// </summary>
    public void SettlePendingRevoke(PendingRevoke retried, PendingRevoke? replacement)
    {
        lock (_lock)
        {
            if (_data.PendingRevoke is not { } current || current.Refresh != retried.Refresh) return;
            _data.PendingRevoke = replacement;
            Save();
        }
    }
}
