using System.ComponentModel;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.App.Views.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class SettingsPage : Page, INotifyPropertyChanged
{
    public SettingsViewModel ViewModel { get; } = new(App.Current.Bridge, App.Current.RunAtLogin);
    public ClipSettingsViewModel ClipSettings { get; } = new(App.Current.Bridge, App.Current.Clipboard, App.Current.Preferences, App.Current.SendTo);

    private string? _recoveryError;
    /// <summary>The last account reset failure, or null.</summary>
    public string? RecoveryError
    {
        get => _recoveryError;
        private set { _recoveryError = value; PropertyChanged?.Invoke(this, new(nameof(RecoveryError))); }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public SettingsPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Preferences");
        Loaded += async (_, _) => await ClipSettings.LoadAsync();
    }

    private async void OnSignOut(object sender, RoutedEventArgs e)
    {
        if (await Dialogs.ConfirmAsync(XamlRoot, "Sign out of fsync?",
                "This PC stops syncing your clipboard and files until you sign in again.", "Sign out"))
            await ViewModel.SignOutCommand.ExecuteAsync(null);
    }

    /// <summary>"Reset account...": two confirmations, then this PC is the only trusted device.</summary>
    private async void OnResetAccount(object sender, RoutedEventArgs e)
    {
        RecoveryError = null;
        // The list may be stale: refresh it. The view model is shared with the Devices page, so a failure
        // here must not leave its error banner behind; the previous Error comes back (a success clears it).
        var devices = App.Current.Devices;
        var previousError = devices.Error;
        try
        {
            await devices.RefreshCommand.ExecuteAsync(null);
            if (devices.Error is not null) devices.Error = previousError;
        }
        catch (Exception ex)
        {
            devices.Error = previousError;
            AppLog.Warn($"settings: device refresh before reset failed {ex.GetType().Name}");
        }
        var others = devices.Devices.Where(d => !d.IsSelf).Select(d => d.Name).ToList();
        RecoveryError = await AccountRecoveryFlow.RunAsync(App.Current.Recovery, App.Current.Bridge, XamlRoot, others);
    }
}
