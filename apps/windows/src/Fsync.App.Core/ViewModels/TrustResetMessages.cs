using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>How the user's last answer to a trust reset ended.</summary>
public enum TrustResetOutcome
{
    /// <summary>No answer given yet.</summary>
    Unanswered,
    /// <summary>The new device list is now trusted (<c>Ready</c>, or <c>Created</c>).</summary>
    Trusted,
    /// <summary>Trusted, but this PC is not in the new list: it must be paired again.</summary>
    RemovedNeedsPairing,
    /// <summary>The reset was declined; the current devices stay trusted.</summary>
    Declined,
}

/// <summary>What the window's trust gate shows.</summary>
public enum TrustGateState
{
    /// <summary>Normal routing.</summary>
    Open,
    /// <summary>Gated: the first check after Ready is still running (progress ring only).</summary>
    Checking,
    /// <summary>Gated: the prompt (pending reset, answer in flight, outcome or message).</summary>
    Prompt,
}

/// <summary>Pure decision behind the trust gate (MainWindow.TrustGate), so it can be unit tested.</summary>
public static class TrustGatePolicy
{
    /// <param name="ready">The account phase is Ready (Offline and others never gate).</param>
    /// <param name="trustChecked">The first check after Ready finished; false also while the view model does not exist.</param>
    /// <param name="currentlyGated">The gate is up now: busy and message only hold it, they never open it.</param>
    public static TrustGateState Decide(bool ready, bool trustChecked, bool hasPending, TrustResetOutcome outcome,
        bool currentlyGated, bool isBusy, bool hasMessage)
    {
        if (!ready) return TrustGateState.Open;
        if (!trustChecked) return TrustGateState.Checking;
        var gated = hasPending
            || outcome is TrustResetOutcome.Trusted or TrustResetOutcome.Declined
            // HasPending drops before Outcome is set while an answer runs; a message stays until dismissed.
            || currentlyGated && (isBusy || hasMessage);
        return gated ? TrustGateState.Prompt : TrustGateState.Open;
    }
}

/// <summary>Text for the trust-reset prompt; <see cref="IsNotice"/> marks informational text (not a failure).</summary>
public readonly record struct TrustResetMessage(string Text, bool IsNotice);

/// <summary>Pure mappings from engine results to what the prompt shows (spec copy, macOS TrustResetMapping).</summary>
public static class TrustResetMessages
{
    public static readonly TrustResetMessage ListChanged = new("The device list changed. Review it again.", true);
    public static readonly TrustResetMessage NotFound = new("This request changed or was already answered. Review the latest one.", true);
    public static readonly TrustResetMessage Network = new("Couldn't reach the server. Check your connection and try again.", false);
    public static readonly TrustResetMessage SignIn = new("Please sign in again, then retry.", false);
    public static readonly TrustResetMessage Other = new("Something went wrong. Please try again.", false);

    /// <summary>Short user-readable text for an engine error. Details are never shown.</summary>
    public static TrustResetMessage For(Exception e) => e switch
    {
        FsyncException.NotFound => NotFound,
        FsyncException.Network => Network,
        FsyncException.Auth or FsyncException.ReauthRequired => SignIn,
        _ => Other,
    };

    /// <summary>Result of <c>ConfirmTrustReset</c>. <c>Created</c> is not expected here and counts as trusted.</summary>
    public static TrustResetOutcome OutcomeFor(AccountState state) =>
        state == AccountState.NeedsPairing ? TrustResetOutcome.RemovedNeedsPairing : TrustResetOutcome.Trusted;
}
