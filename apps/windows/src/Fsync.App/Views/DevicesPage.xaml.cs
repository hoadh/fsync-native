using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class DevicesPage : Page
{
    public DevicesViewModel ViewModel { get; } = App.Current.Devices;
    public EngineBridge Bridge { get; } = App.Current.Bridge;

    public DevicesPage()
    {
        InitializeComponent();
        Loaded += async (_, _) => await ViewModel.RefreshCommand.ExecuteAsync(null);
    }

    private async void OnRetry(object sender, RoutedEventArgs e) => await Bridge.GoOnlineAsync();

    private async void OnRemove(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not DeviceItem item) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = $"Remove {item.Name}?",
            Content = "It will stop receiving your clipboard and files. To use it again, pair it again.",
            PrimaryButtonText = "Remove",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.RemoveCommand.ExecuteAsync(item);
    }
}
