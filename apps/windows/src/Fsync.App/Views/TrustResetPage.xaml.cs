using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

/// <summary>
/// Answer to another device's trust reset. Keep (default, focused) declines; Confirm opens a
/// destructive dialog (Close is the default) bound to the reset id it was opened for. When the shown
/// reset changes while the dialog is open, the view model raises ConfirmAborted and the dialog closes.
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
        Unloaded += OnUnloaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        ViewModel.ConfirmAborted += OnConfirmAborted;
        KeepButton.Focus(FocusState.Programmatic);
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        ViewModel.ConfirmAborted -= OnConfirmAborted;
        _dialog?.Hide(); // the gate closed under an open dialog: it answers nothing
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
        if (ViewModel.Current is { } reset) await ViewModel.DeclineAsync(reset.resetId);
    }

    private async void OnConfirm(object sender, RoutedEventArgs e)
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

    private void OnConfirmAborted() => _dialog?.Hide();

    private void OnContinue(object sender, RoutedEventArgs e) => ViewModel.DismissOutcome();
}
