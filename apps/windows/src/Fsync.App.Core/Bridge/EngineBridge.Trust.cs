using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Account state changes that come from trust calls (trust reset confirm, account recovery).</summary>
public sealed partial class EngineBridge
{
    /// <summary>Shown on the sign-in screen when a trust call needs a fresh login.</summary>
    public const string SignInAgainMessage = "Sign in again to continue.";

    /// <summary>
    /// Maps the state a trust call returned to a phase, the same way as <see cref="GoOnlineAsync"/>:
    /// <c>NeedsPairing</c> → NeedsPairing, <c>Ready</c>/<c>Created</c> → Ready. Only from Ready or NeedsPairing:
    /// a late result must not overwrite SignedOut, Offline, Starting or Failed. Returns false when ignored. UI thread.
    /// </summary>
    public bool ApplyAccountState(AccountState state)
    {
        if (Phase is not (AccountPhase.Ready or AccountPhase.NeedsPairing))
        {
            AppLog.Info($"bridge: account state {state} ignored in phase {Phase}");
            return false;
        }
        Phase = state == AccountState.NeedsPairing ? AccountPhase.NeedsPairing : AccountPhase.Ready;
        StatusMessage = null;
        DevicesChanged?.Invoke();
        return true;
    }

    /// <summary>The server wants a fresh login (<c>Auth</c>, <c>ReauthRequired</c>): same path as SessionExpired.</summary>
    public void RequireSignIn()
    {
        try { _secrets.ClearSession(); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Security.Cryptography.CryptographicException)
        {
            // The next launch lands signed out on Auth anyway.
            AppLog.Warn($"bridge: could not clear the session: {e.GetType().Name}");
        }
        Phase = AccountPhase.SignedOut;
        StatusMessage = SignInAgainMessage;
        DevicesChanged?.Invoke();
    }
}
