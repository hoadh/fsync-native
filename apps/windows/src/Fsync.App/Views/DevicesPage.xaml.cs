using System.Collections.Specialized;
using System.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;

namespace Fsync.App.Views;

public sealed partial class DevicesPage : Page, INotifyPropertyChanged
{
    public DevicesViewModel ViewModel { get; } = App.Current.Devices;
    public EngineBridge Bridge { get; } = App.Current.Bridge;

    private bool _loaded;
    /// <summary>After the first refresh, only this PC is listed (spec: devices.empty + Pair CTA).</summary>
    public bool ShowEmpty => _loaded && !ViewModel.Devices.Any(d => !d.IsSelf);

    public event PropertyChangedEventHandler? PropertyChanged;

    public DevicesPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Your devices", "Drop files on a device to send them.");
        AddExtraBanners(ExtraBannerHost);
        Loaded += async (_, _) =>
        {
            ViewModel.Devices.CollectionChanged -= OnDevicesChanged; // Loaded can repeat without a new page
            ViewModel.Devices.CollectionChanged += OnDevicesChanged;
            await ViewModel.RefreshCommand.ExecuteAsync(null);
            _loaded = true;
            OnDevicesChanged(null, null);
        };
        Unloaded += (_, _) => ViewModel.Devices.CollectionChanged -= OnDevicesChanged;
    }

    /// <summary>Phase 11 seam: extra InfoBars (first device, offline hints) above the list.</summary>
    partial void AddExtraBanners(StackPanel host);

    private void OnDevicesChanged(object? sender, NotifyCollectionChangedEventArgs? e) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(ShowEmpty)));

    private void OnDragOver(object sender, DragEventArgs e)
    {
        if (!e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        e.AcceptedOperation = DataPackageOperation.Copy;
        if ((sender as FrameworkElement)?.Tag is DeviceItem d) e.DragUIOverride.Caption = $"Send to {PeerText.DisplayName(d.Name)}";
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

    private void OnPairNow(object sender, RoutedEventArgs e) => App.Current.ShowWindow(MainWindow.PairingTag);

    private async void OnRemove(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not DeviceItem item) return;
        // The answer applies to the row the dialog opened for (captured item), never the current one.
        if (await Dialogs.ConfirmAsync(XamlRoot, $"Remove {PeerText.DisplayName(item.Name)}?",
                "It will stop receiving your clipboard and files. To use it again, pair it again.", "Remove"))
            await ViewModel.RemoveCommand.ExecuteAsync(item);
    }
}
