using Fsync.App.ViewModels;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class SettingsPage : Page
{
    public SettingsViewModel ViewModel { get; } = new(App.Current.Bridge, App.Current.RunAtLogin);
    public ClipSettingsViewModel ClipSettings { get; } = new(App.Current.Bridge, App.Current.Clipboard, App.Current.Preferences, App.Current.SendTo);

    public SettingsPage()
    {
        InitializeComponent();
        Loaded += async (_, _) => await ClipSettings.LoadAsync();
    }
}
