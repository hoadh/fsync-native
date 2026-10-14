using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>Settings shell for phase 13: account, this device, start with Windows, sign out.</summary>
public sealed partial class SettingsViewModel : ObservableObject
{
    private readonly EngineBridge _bridge;
    private readonly IRunAtLogin _runAtLogin;

    [ObservableProperty] private bool _startWithWindows;
    [ObservableProperty] private string? _error;
    private bool _syncing;

    public SettingsViewModel(EngineBridge bridge, IRunAtLogin runAtLogin)
    {
        _bridge = bridge;
        _runAtLogin = runAtLogin;
        Sync();
    }

    /// <summary>Shows the registry's actual value without writing it back.</summary>
    private void Sync()
    {
        _syncing = true;
        try { StartWithWindows = _runAtLogin.IsEnabled; }
        finally { _syncing = false; }
    }

    public string Email => _bridge.Email ?? "";
    public string DeviceName => _bridge.DeviceName;
    public string NodeId => _bridge.NodeId;

    partial void OnStartWithWindowsChanged(bool value)
    {
        if (_syncing) return;
        try
        {
            _runAtLogin.Set(value);
            Error = null;
        }
        catch (Exception e) when (e is UnauthorizedAccessException or IOException or System.Security.SecurityException)
        {
            Error = $"Could not change the startup setting: {e.Message}";
            Sync();
        }
    }

    [RelayCommand]
    private async Task SignOutAsync()
    {
        try { await _bridge.LogoutAsync(); }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }
}
