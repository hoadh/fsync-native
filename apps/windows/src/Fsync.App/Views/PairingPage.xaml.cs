using System.ComponentModel;
using System.Runtime.InteropServices.WindowsRuntime;
using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage.Streams;

namespace Fsync.App.Views;

/// <summary>Offer (new device) or join (existing device); the SAS step is shared.</summary>
public sealed partial class PairingPage : Page, INotifyPropertyChanged
{
    public PairingViewModel ViewModel { get; } = App.Current.Pairing;
    public bool IsNewDevice => App.Current.Bridge.Phase == AccountPhase.NeedsPairing;
    public bool IsExistingDevice => !IsNewDevice;

    public event PropertyChangedEventHandler? PropertyChanged;

    public PairingPage()
    {
        InitializeComponent();
        ViewModel.PropertyChanged += OnViewModelChanged;
        App.Current.Bridge.PropertyChanged += OnBridgeChanged;
        Unloaded += (_, _) =>
        {
            ViewModel.PropertyChanged -= OnViewModelChanged;
            App.Current.Bridge.PropertyChanged -= OnBridgeChanged;
        };
    }

    private void OnBridgeChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(EngineBridge.Phase)) return;
        PropertyChanged?.Invoke(this, new(nameof(IsNewDevice)));
        PropertyChanged?.Invoke(this, new(nameof(IsExistingDevice)));
    }

    private async void OnViewModelChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(PairingViewModel.QrPng)) return;
        if (ViewModel.QrPng is not { } png) { QrImage.Source = null; return; }
        using var stream = new InMemoryRandomAccessStream();
        await stream.WriteAsync(png.AsBuffer());
        stream.Seek(0);
        var bmp = new BitmapImage();
        await bmp.SetSourceAsync(stream);
        QrImage.Source = bmp;
    }

    private async void OnOfferCode(object sender, RoutedEventArgs e) => await ViewModel.OfferCommand.ExecuteAsync(PairingMode.Code);
    private async void OnOfferQr(object sender, RoutedEventArgs e) => await ViewModel.OfferCommand.ExecuteAsync(PairingMode.Qr);
}
