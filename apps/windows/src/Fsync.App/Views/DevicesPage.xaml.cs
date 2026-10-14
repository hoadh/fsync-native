using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;

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

    private void OnDragOver(object sender, DragEventArgs e)
    {
        if (!e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        e.AcceptedOperation = DataPackageOperation.Copy;
        if ((sender as FrameworkElement)?.Tag is DeviceItem d) e.DragUIOverride.Caption = $"Send to {d.Name}";
    }

    /// <summary>Files dropped on a device row are pushed to it ("Send to").</summary>
    private async void OnDrop(object sender, DragEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not DeviceItem d || !e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        var deferral = e.GetDeferral();
        try
        {
            var items = await e.DataView.GetStorageItemsAsync();
            var paths = items.Select(i => i.Path).Where(p => !string.IsNullOrEmpty(p)).ToList();
            if (await App.Current.Transfers.SendAsync(d.NodeId, d.Name, paths)) App.Current.ShowWindow(MainWindow.FilesTag);
            else ViewModel.Error = App.Current.Transfers.Error;
        }
        finally { deferral.Complete(); }
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
        if (await Dialogs.ShowAsync(dialog) == ContentDialogResult.Primary) await ViewModel.RemoveCommand.ExecuteAsync(item);
    }
}
