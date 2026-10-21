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

    /// <summary>A refresh on this page succeeded at least once.</summary>
    private bool _loadedOk;
    private bool _updateQueued;
    private bool _showEmpty;

    /// <summary>
    /// Only this PC is listed after a successful load while online and error-free (spec: devices.empty +
    /// Pair CTA). Not shown while offline or after a failed refresh: the list is unknown then.
    /// </summary>
    public bool ShowEmpty
    {
        get => _showEmpty;
        private set
        {
            if (_showEmpty == value) return;
            _showEmpty = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(ShowEmpty)));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public DevicesPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Your devices", "Drop files on a device to send them.");
        AddExtraBanners(ExtraBannerHost);
        Loaded += OnLoaded;
        Unloaded += (_, _) => Unsubscribe();
    }

    /// <summary>Phase 11 seam: extra InfoBars (first device, offline hints) above the list.</summary>
    partial void AddExtraBanners(StackPanel host);

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            Unsubscribe(); // Loaded can repeat without a new page
            ViewModel.Devices.CollectionChanged += OnDevicesChanged;
            ViewModel.PropertyChanged += OnStateChanged;
            Bridge.PropertyChanged += OnStateChanged;
            await ViewModel.RefreshCommand.ExecuteAsync(null);
            _loadedOk |= ViewModel.Error is null;
            UpdateShowEmpty();
        }
        catch (Exception ex)
        {
            AppLog.Warn($"devices: page load failed {ex.GetType().Name}");
        }
    }

    private void Unsubscribe()
    {
        ViewModel.Devices.CollectionChanged -= OnDevicesChanged;
        ViewModel.PropertyChanged -= OnStateChanged;
        Bridge.PropertyChanged -= OnStateChanged;
    }

    private void OnDevicesChanged(object? sender, NotifyCollectionChangedEventArgs e) => QueueUpdate();

    private void OnStateChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(DevicesViewModel.Error) or nameof(EngineBridge.Phase))) return;
        // Error cleared by a refresh (e.g. after a failed first load): that refresh succeeded.
        if (sender == ViewModel && ViewModel.Error is null) _loadedOk = true;
        QueueUpdate();
    }

    /// <summary>
    /// A refresh replaces the list with Clear + Add in one UI-thread turn; deciding after that turn
    /// avoids a flash of the empty state in between.
    /// </summary>
    private void QueueUpdate()
    {
        if (_updateQueued) return;
        _updateQueued = DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () =>
        {
            _updateQueued = false;
            UpdateShowEmpty();
        });
    }

    private void UpdateShowEmpty() =>
        ShowEmpty = _loadedOk && ViewModel.Error is null && Bridge.Phase == AccountPhase.Ready
            && !ViewModel.Devices.Any(d => !d.IsSelf);

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
