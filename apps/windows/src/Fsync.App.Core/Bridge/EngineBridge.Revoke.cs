using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Hard logout after quit: see <see cref="PendingRevoke"/>.</summary>
public sealed partial class EngineBridge
{
    /// <summary>
    /// Sign-out half that talks to the backend. Order matters: persist the pending revoke BEFORE
    /// <c>LogoutConfirmed</c>, so a quit during the call still leaves a retry for the next launch.
    /// </summary>
    private async Task LogoutKeepingRevokeAsync()
    {
        var engine = _engine;
        await Task.Run(() =>
        {
            var saved = SavePendingRevoke();
            if (engine is null)
            {
                // No engine to log out with (boot failed): revoke directly.
                if (saved is not null) RunPendingRevoke();
                return;
            }
            try
            {
                // Only drop the entry this sign-out saved; a newer one stays for the next launch.
                if (engine.LogoutConfirmed() && saved is not null) _secrets.SettlePendingRevoke(saved, null);
            }
            catch (FsyncException e)
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

    /// <summary>The newest refresh token the store holds (every rotation is stored by the delegate).</summary>
    private PendingRevoke? SavePendingRevoke()
    {
        try
        {
            if (_secrets.Session is not { } session) return null;
            var pending = new PendingRevoke(session.refresh, _settings.ApiBase, DateTimeOffset.UtcNow);
            _secrets.SavePendingRevoke(pending);
            return pending;
        }
        catch (Exception e) when (e is IOException or System.Security.Cryptography.CryptographicException)
        {
            AppLog.Warn($"bridge: could not keep a pending revoke: {e.GetType().Name}");
            return null;
        }
    }
}
