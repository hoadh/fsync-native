using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;

namespace Fsync.App.Views;

/// <summary>
/// Answer to another device's trust reset. Keep (default, focused) declines; Confirm opens a
/// destructive dialog (Close is the default) bound to the reset id it was opened for. When the shown
/// reset changes while the dialog is open, the view model raises ConfirmAborted and the dialog closes.
/// While the first check after Ready runs, the page shows only a progress ring (<see cref="SetChecking"/>).
/// The view model lives as long as the app: every exit path (navigation, unload, window close) detaches.
/// </summary>
public sealed partial class TrustResetPage : Page
{
    /// <summary>Only navigated to by the trust gate, which requires the view model.</summary>
    public TrustResetPromptViewModel ViewModel { get; } = App.Current.TrustReset!;
    private ContentDialog? _dialog;

    public TrustResetPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += (_, _) => Detach();
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        ViewModel.ConfirmAborted -= OnConfirmAborted; // Loaded may fire more than once
        ViewModel.ConfirmAborted += OnConfirmAborted;
        if (KeepButton.IsEnabled && ContentPanel.Visibility == Visibility.Visible) KeepButton.Focus(FocusState.Programmatic);
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        base.OnNavigatedFrom(e);
        Detach();
    }

    /// <summary>Unsubscribes from the view model and closes an open dialog (it then answers nothing). Idempotent.</summary>
    internal void Detach()
    {
        ViewModel.ConfirmAborted -= OnConfirmAborted;
        _dialog?.Hide();
    }

    /// <summary>Checking: only the ring shows. Afterwards the prompt shows and Keep gets the focus.</summary>
    internal void SetChecking(bool checking)
    {
        var wasChecking = CheckingPanel.Visibility == Visibility.Visible;
        CheckingPanel.Visibility = checking ? Visibility.Visible : Visibility.Collapsed;
        CheckingRing.IsActive = checking;
        ContentPanel.Visibility = checking ? Visibility.Collapsed : Visibility.Visible;
        if (wasChecking && !checking && KeepButton.IsEnabled) KeepButton.Focus(FocusState.Programmatic);
    }

    // x:Bind functions on the page: instance members (the generated code calls them on the page).
    public bool Idle(bool busy) => !busy;

    public InfoBarSeverity MessageSeverity(bool isNotice) => isNotice ? InfoBarSeverity.Informational : InfoBarSeverity.Error;

    public string OutcomeText(TrustResetOutcome outcome) => outcome switch
    {
        TrustResetOutcome.Trusted => "The new device list is trusted.",
        TrustResetOutcome.Declined => "This PC keeps its current devices.",
        _ => "",
    };

    public string NewListTitle(TrustResetInfo? reset) => reset is null ? ""
        : reset.devices == 1 ? "New list: 1 device" : $"New list: {reset.devices} devices";

    private async void OnKeep(object sender, RoutedEventArgs e)
    {
        try
        {
            if (ViewModel.Current is { } reset) await ViewModel.DeclineAsync(reset.resetId);
        }
        catch (Exception ex) { AppLog.Warn($"trust: keep failed {ex.GetType().Name}"); }
    }

    private async void OnConfirm(object sender, RoutedEventArgs e)
    {
        try
        {
            if (ViewModel.Current is not { } reset) return;
            var id = reset.resetId; // the dialog answers exactly the reset the user saw
            if (!ViewModel.BeginConfirm(id)) return;
            bool confirmed;
            try
            {
                confirmed = await Dialogs.ConfirmAsync(XamlRoot, "Trust the new device list?",
                    "Devices that are not in the new list can no longer sync with this PC. Confirm only if you reset the list yourself.",
                    "Confirm reset", d => _dialog = d);
            }
            finally { _dialog = null; }
            if (confirmed) await ViewModel.ConfirmAsync(id); // a no-op when the dialog was aborted
            else ViewModel.CancelConfirm();
        }
        catch (Exception ex)
        {
            AppLog.Warn($"trust: confirm failed {ex.GetType().Name}");
            ViewModel.CancelConfirm(); // never leave ConfirmingResetId set
        }
    }

    private void OnConfirmAborted() => _dialog?.Hide();

    /// <summary>Hides the outcome and any message; the gate closes when nothing is pending.</summary>
    private void OnDismiss(object sender, RoutedEventArgs e) => ViewModel.DismissOutcome();
}
