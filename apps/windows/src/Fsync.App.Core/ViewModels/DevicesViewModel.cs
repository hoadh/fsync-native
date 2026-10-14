using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>Paired devices with live status (on_device_status) and remove.</summary>
public sealed partial class DevicesViewModel : ObservableObject, IDisposable
{
    private readonly EngineBridge _bridge;
    private readonly Func<DateTimeOffset> _now;

    public ObservableCollection<DeviceItem> Devices { get; } = [];
    [ObservableProperty] private string? _error;
    [ObservableProperty] private bool _isBusy;

    public DevicesViewModel(EngineBridge bridge, Func<DateTimeOffset>? now = null)
    {
        _bridge = bridge;
        _now = now ?? (() => DateTimeOffset.Now);
        _bridge.DeviceStatusChanged += OnStatus;
        _bridge.DevicesChanged += OnChanged;
    }

    /// <summary>Other devices that are connected right now (for the tray badge).</summary>
    public int ConnectedPeers => Devices.Count(d => !d.IsSelf && d.Kind == StatusKind.Good);

    [RelayCommand]
    public async Task RefreshAsync()
    {
        try
        {
            var list = await _bridge.RunAsync(e => e.Devices());
            Replace(list.OrderByDescending(d => d.isSelf).ThenBy(d => d.name, StringComparer.CurrentCultureIgnoreCase));
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    /// <summary>The view asks for confirmation before calling this.</summary>
    [RelayCommand]
    public async Task RemoveAsync(DeviceItem item)
    {
        if (!item.CanRemove) return;
        IsBusy = true;
        try
        {
            await _bridge.RunAsync(e => e.RemoveDevice(item.NodeId));
            await RefreshAsync();
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        finally { IsBusy = false; }
    }

    private void OnStatus(Device d)
    {
        var item = DeviceItem.From(d, _now());
        var i = IndexOf(d.nodeId);
        if (i >= 0) Devices[i] = item;
        else Devices.Add(item);
        OnPropertyChanged(nameof(ConnectedPeers));
    }

    private void OnChanged() => _ = RefreshAsync();

    private void Replace(IEnumerable<Device> list)
    {
        Devices.Clear();
        foreach (var d in list) Devices.Add(DeviceItem.From(d, _now()));
        OnPropertyChanged(nameof(ConnectedPeers));
    }

    private int IndexOf(string nodeId)
    {
        for (var i = 0; i < Devices.Count; i++) if (Devices[i].NodeId == nodeId) return i;
        return -1;
    }

    public void Dispose()
    {
        _bridge.DeviceStatusChanged -= OnStatus;
        _bridge.DevicesChanged -= OnChanged;
    }
}
