using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Hard logout after quit: see <see cref="PendingRevoke"/>.</summary>
public sealed partial class EngineBridge
{
    /// <summary>
    /// Signs out; does not throw on backend or secret-store trouble. Whatever happens while talking to the backend or the secret store, the local session
    /// is cleared: the pending revoke (if it was saved) is what retries the server side.
    /// </summary>
    public async Task LogoutAsync()
    {
        try { await LogoutKeepingRevokeAsync(); }
        finally
        {
            try { _secrets.ClearSession(); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Security.Cryptography.CryptographicException)
            {
                // Tokens stay on disk but are revoked or queued for revoke; the next launch lands signed out on Auth.
                AppLog.Warn($"bridge: could not clear the session: {e.GetType().Name}");
            }
            Phase = AccountPhase.SignedOut;
            DevicesChanged?.Invoke();
        }
    }

    /// <summary>
    /// Backend half. Order matters: persist the pending revoke BEFORE <c>LogoutConfirmed</c>, so a quit
    /// during the call still leaves a retry for the next launch.
    /// </summary>
    private async Task LogoutKeepingRevokeAsync()
    {
        var engine = _engine;
        await Task.Run(() =>
        {
            var saved = SavePendingRevoke();
            try
            {
                if (engine is null)
                {
                    // No engine to log out with (boot failed): revoke directly.
                    if (saved is not null) RunPendingRevoke();
                    return;
                }
                // Only drop the entry this sign-out saved; a newer one stays for the next launch.
                // A failed settle is harmless: the retry gets 401, which counts as revoked.
                if (engine.LogoutConfirmed() && saved is not null) _secrets.SettlePendingRevoke(saved, null);
            }
            catch (Exception e)
            {
                AppLog.Warn($"bridge: logout failed: {e.GetType().Name}");
            }
        });
    }

    /// <summary>Retries a revoke left by an earlier sign-out. Off the UI thread, never blocks startup.</summary>
    private void ResumePendingRevoke() => _ = Task.Run(RunPendingRevoke);

    private void RunPendingRevoke()
    {
        try
        {
            var outcome = PendingRevokeResume.Run(_secrets, DateTimeOffset.UtcNow, _revoke);
            if (outcome != PendingRevokeResume.Outcome.NothingPending) AppLog.Info($"bridge: pending revoke {outcome}");
        }
        catch (Exception e)
        {
            // Native library or secret store trouble: a failed retry must never take the app down.
            AppLog.Warn($"bridge: pending revoke failed: {e.GetType().Name}");
        }
    }

    /// <summary>Keeps the newest refresh token (every rotation is stored by the delegate). Null if it could not.</summary>
    private PendingRevoke? SavePendingRevoke()
    {
        try { return _secrets.SavePendingRevokeFromSession(_settings.ApiBase, DateTimeOffset.UtcNow); }
        catch (Exception e)
        {
            AppLog.Warn($"bridge: could not keep a pending revoke: {e.GetType().Name}");
            return null;
        }
    }
}
