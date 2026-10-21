using System.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views.Controls;

/// <summary>
/// "Lost your other devices?" (ADR 012, spec recovery.* copy): drives <see cref="AccountRecoveryViewModel"/>
/// Explain → FinalConfirm through two <see cref="Dialogs.ConfirmAsync"/> (Enter / Esc = Cancel), then
/// <c>ResetAsync</c>. Nothing proceeds on its own. Both dialogs close when the account phase changes
/// while they are open (spec: destructive dialog rule), and the flow then stops. A session that needs
/// re-auth lands on the sign-in screen (the view model calls <c>RequireSignIn</c>).
/// </summary>
internal static class AccountRecoveryFlow
{
    private const string ExplainBody =
        "If you can't pair with another device, this PC can become the only trusted device on your account. " +
        "Every other device you own will have to confirm the change or pair again before it syncs with you.";

    private const string FinalBody =
        "Your other devices will stop syncing until you confirm the reset on them or pair them again. " +
        "This can't be undone, and it can be done only a few times a day.";

    /// <summary>At most this many device names are listed in the final dialog.</summary>
    private const int MaxListedNames = 8;

    /// <param name="recovery"><c>App.Recovery</c>; null (engine not ready) does nothing.</param>
    /// <param name="otherDeviceNames">
    /// Peer-supplied names of the devices that lose trust, shown sanitized; empty shows a general sentence
    /// instead, null (no list to show, e.g. a PC that needs pairing) shows neither.
    /// </param>
    /// <returns>The failure text to show, or null (done, declined, or interrupted).</returns>
    public static async Task<string?> RunAsync(AccountRecoveryViewModel? recovery, EngineBridge bridge, XamlRoot root,
        IReadOnlyList<string>? otherDeviceNames)
    {
        if (recovery is null || recovery.IsBusy) return null;
        var phase = bridge.Phase;
        ContentDialog? open = null;
        void OnPhase(object? sender, PropertyChangedEventArgs e)
        {
            if (e.PropertyName == nameof(EngineBridge.Phase) && bridge.Phase != phase) open?.Hide();
        }

        bridge.PropertyChanged += OnPhase;
        try
        {
            recovery.Begin();
            if (!await Dialogs.ConfirmAsync(root, "Lost your other devices?", ExplainBody, "Continue...", d => open = d)
                || bridge.Phase != phase)
                return null;

            recovery.Proceed();
            if (!await Dialogs.ConfirmAsync(root, "Reset trust for your account?", FinalBody + DeviceList(otherDeviceNames),
                    "Reset and trust only this PC", d => open = d)
                || bridge.Phase != phase)
                return null;

            open = null;
            await recovery.ResetAsync();
            return recovery.Error;
        }
        finally
        {
            bridge.PropertyChanged -= OnPhase;
            recovery.Cancel(); // back to Idle after a decline or failure; a no-op once the reset ran
        }
    }

    /// <summary>"\n\nDevices that will need to ...: ..." with sanitized names; a general sentence when the list is empty.</summary>
    internal static string DeviceList(IReadOnlyList<string>? names)
    {
        if (names is null) return "";
        var shown = names.Select(n => PeerText.DisplayName(n)).ToList();
        if (shown.Count == 0) return "\n\nYour other devices will need to confirm or pair again.";
        var listed = shown.Take(MaxListedNames).Select(n => "• " + n);
        var more = shown.Count > MaxListedNames ? $"\n…and {shown.Count - MaxListedNames} more" : "";
        return "\n\nDevices that will need to confirm or pair again:\n" + string.Join("\n", listed) + more;
    }
}
