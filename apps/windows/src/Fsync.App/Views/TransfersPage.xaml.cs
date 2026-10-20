using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.Diagnostics;
using Fsync.App.Files;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

/// <summary>Send-to device picker, pushed-file offers, transfer list.</summary>
public sealed partial class TransfersPage : Page, INotifyPropertyChanged
{
    public TransfersViewModel ViewModel { get; } = App.Current.Transfers;
    /// <summary>Paired devices other than this PC (send targets).</summary>
    public ObservableCollection<SendTarget> Targets { get; } = [];
    public bool HasOffers => ViewModel.Offers.Count > 0;
    /// <summary>No transfers in the list (spec: files.empty).</summary>
    public bool ShowEmpty => ViewModel.Transfers.Count == 0;

    public event PropertyChangedEventHandler? PropertyChanged;

    public TransfersPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Files and transfers");
        SetupFileDrop(DropHost);
        ViewModel.Offers.CollectionChanged += OnOffersChanged;
        ViewModel.Transfers.CollectionChanged += OnTransfersChanged;
        Loaded += async (_, _) => await ShowSendAsync();
        Unloaded += (_, _) =>
        {
            ViewModel.Offers.CollectionChanged -= OnOffersChanged;
            ViewModel.Transfers.CollectionChanged -= OnTransfersChanged;
        };
    }

    /// <summary>Phase 11 seam: files dropped anywhere on the page go to a picked device.</summary>
    partial void SetupFileDrop(UIElement host);

    private void OnOffersChanged(object? sender, NotifyCollectionChangedEventArgs e) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(HasOffers)));

    private void OnTransfersChanged(object? sender, NotifyCollectionChangedEventArgs e) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(ShowEmpty)));

    /// <summary>Shows the device picker while Explorer paths are queued.</summary>
    internal async Task ShowSendAsync()
    {
        var paths = App.Current.PendingSend;
        SendPanel.Visibility = paths.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        if (paths.Count == 0) return;
        SendWhat.Text = paths.Count == 1 ? Path.GetFileName(paths[0].TrimEnd('\\')) : $"{paths.Count} items";
        await App.Current.Devices.RefreshAsync();
        Targets.Clear();
        foreach (var d in App.Current.Devices.Devices.Where(d => !d.IsSelf)) Targets.Add(new SendTarget(d.NodeId, d.Name, d.StatusText));
    }

    private async void OnSendTo(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not SendTarget d) return;
        if (await ViewModel.SendAsync(d.NodeId, d.Name, App.Current.PendingSend)) OnCancelSend(sender, e);
    }

    private void OnCancelSend(object sender, RoutedEventArgs e)
    {
        App.Current.PendingSend.Clear();
        SendPanel.Visibility = Visibility.Collapsed;
    }

    private static T? TagOf<T>(object sender) where T : class => (sender as FrameworkElement)?.Tag as T;

    private async void OnAccept(object sender, RoutedEventArgs e)
    {
        if (TagOf<IncomingOffer>(sender) is { } o) await ViewModel.AcceptAsync(o);
    }

    private void OnDecline(object sender, RoutedEventArgs e)
    {
        if (TagOf<IncomingOffer>(sender) is { } o) ViewModel.Decline(o);
    }

    private async void OnCancel(object sender, RoutedEventArgs e)
    {
        if (TagOf<TransferItem>(sender) is { } t) await ViewModel.CancelAsync(t);
    }

    private async void OnRetry(object sender, RoutedEventArgs e)
    {
        if (TagOf<TransferItem>(sender) is { } t) await ViewModel.RetryAsync(t);
    }

    private void OnShow(object sender, RoutedEventArgs e)
    {
        if (TagOf<TransferItem>(sender)?.Path is not { } p || !File.Exists(p)) return;
        try { Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{p}\"") { UseShellExecute = true }); }
        catch (Win32Exception) { }
    }
}

/// <summary>A device button in the Send-to picker (get-only: the XAML compiler cannot set records).</summary>
public sealed class SendTarget(string nodeId, string name, string statusText)
{
    public string NodeId { get; } = nodeId;
    public string Name { get; } = name;
    public string StatusText { get; } = statusText;
    public override string ToString() => $"{Name}, {StatusText}";
}
