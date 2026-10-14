using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.App.Bridge;

namespace Fsync.App.ViewModels;

/// <summary>Tray badge and tooltip, derived from the account phase and connected devices.</summary>
public sealed partial class TrayStatusViewModel : ObservableObject, IDisposable
{
    private readonly EngineBridge _bridge;
    private readonly DevicesViewModel _devices;

    [ObservableProperty] private string _tooltip = "fsync";
    [ObservableProperty] private StatusKind _kind = StatusKind.Neutral;

    public TrayStatusViewModel(EngineBridge bridge, DevicesViewModel devices)
    {
        _bridge = bridge;
        _devices = devices;
        _bridge.PropertyChanged += OnChanged;
        _devices.PropertyChanged += OnChanged;
        Update();
    }

    private void OnChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(EngineBridge.Phase) or nameof(EngineBridge.StatusMessage) or nameof(DevicesViewModel.ConnectedPeers))
            Update();
    }

    internal void Update()
    {
        (Tooltip, Kind) = Compute(_bridge.Phase, _devices.ConnectedPeers, _bridge.StatusMessage);
    }

    public static (string Tooltip, StatusKind Kind) Compute(AccountPhase phase, int connectedPeers, string? message) => phase switch
    {
        AccountPhase.Starting => ("fsync: starting", StatusKind.Neutral),
        AccountPhase.SignedOut => ("fsync: not signed in", StatusKind.Warning),
        AccountPhase.NeedsPairing => ("fsync: pair this PC with one of your devices", StatusKind.Warning),
        AccountPhase.Offline => ($"fsync: offline{(message is null ? "" : $" ({message})")}", StatusKind.Bad),
        AccountPhase.Failed => ($"fsync: {message ?? "error"}", StatusKind.Bad),
        AccountPhase.Ready when connectedPeers == 0 => ("fsync: no other device connected", StatusKind.Neutral),
        AccountPhase.Ready => ($"fsync: connected to {connectedPeers} device{(connectedPeers == 1 ? "" : "s")}", StatusKind.Good),
        _ => ("fsync", StatusKind.Neutral),
    };

    public void Dispose()
    {
        _bridge.PropertyChanged -= OnChanged;
        _devices.PropertyChanged -= OnChanged;
    }
}
