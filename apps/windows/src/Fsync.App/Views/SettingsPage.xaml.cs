using Fsync.App.ViewModels;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class SettingsPage : Page
{
    public SettingsViewModel ViewModel { get; } = new(App.Current.Bridge, App.Current.RunAtLogin);

    public SettingsPage() => InitializeComponent();
}
