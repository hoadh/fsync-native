using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>
/// General / Privacy / Storage settings. Core keeps mode, retention, size limit, denylist and
/// auto-accept (<see cref="SaveAsync"/>, applied immediately by core); the app keeps the file
/// strategy, hotkeys and the Send To shortcut.
/// </summary>
public sealed partial class ClipSettingsViewModel : ObservableObject
{
    private readonly EngineBridge _bridge;
    private readonly ClipboardService _clipboard;
    private readonly AppPreferences _prefs;
    private readonly SendToShortcut _sendTo;
    private const double MiB = 1024 * 1024, GiB = 1024.0 * 1024 * 1024;
    private Settings? _loaded;

    [ObservableProperty] private bool _manualMode;
    [ObservableProperty] private bool _autoAcceptFiles;
    [ObservableProperty] private double _retentionDays = 30;
    [ObservableProperty] private double _retentionGb = 1;
    [ObservableProperty] private double _maxClipMb = 64;
    /// <summary>One exe name per line (without ".exe").</summary>
    [ObservableProperty] private string _denylist = "";
    [ObservableProperty] private bool _eagerFiles;
    [ObservableProperty] private bool _hotkeys;
    [ObservableProperty] private bool _sendToMenu;
    [ObservableProperty] private string? _error;
    [ObservableProperty] private string? _saved;

    public ClipSettingsViewModel(EngineBridge bridge, ClipboardService clipboard, AppPreferences prefs, SendToShortcut sendTo)
    {
        _bridge = bridge;
        _clipboard = clipboard;
        _prefs = prefs;
        _sendTo = sendTo;
        _eagerFiles = prefs.FileStrategy == FileClipStrategy.Eager;
        _hotkeys = prefs.HotkeysEnabled;
        _sendToMenu = sendTo.IsInstalled;
    }

    [RelayCommand]
    public async Task LoadAsync()
    {
        try
        {
            var s = await _bridge.RunAsync(e => e.Settings());
            _loaded = s;
            ManualMode = s.clipMode == ClipMode.Manual;
            AutoAcceptFiles = s.autoAcceptFiles;
            RetentionDays = s.retentionDays;
            RetentionGb = Math.Round(s.retentionBytes / GiB, 2);
            MaxClipMb = Math.Round(s.maxClipBytes / MiB, 2); // binary units, like core (64 MiB shows 64)
            Denylist = string.Join(Environment.NewLine, s.denylist);
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    /// <summary>The core settings these fields describe (null before <see cref="LoadAsync"/>).</summary>
    public Settings? ToSettings() => _loaded is null ? null : _loaded with
    {
        clipMode = ManualMode ? ClipMode.Manual : ClipMode.Auto,
        autoAcceptFiles = AutoAcceptFiles,
        retentionDays = (uint)Math.Clamp(RetentionDays, 1, 3650),
        retentionBytes = (ulong)Math.Round(Math.Clamp(RetentionGb, 0.01, 1000) * GiB),
        maxClipBytes = (ulong)Math.Round(Math.Clamp(MaxClipMb, 1, 4096) * MiB),
        denylist = [.. Denylist.Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).Distinct(StringComparer.OrdinalIgnoreCase)],
    };

    [RelayCommand]
    public async Task SaveAsync()
    {
        if (ToSettings() is not { } s) return;
        try
        {
            await _bridge.RunAsync(e => e.SetSettings(s));
            _loaded = s;
            await _clipboard.RefreshSettingsAsync();
            Saved = "Saved.";
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
    }

    /// <summary>Restores the default password-manager denylist (not saved until Save).</summary>
    [RelayCommand]
    public void ResetDenylist() => Denylist = string.Join(Environment.NewLine, Interop.Clipboard.SensitiveFormatFilter.DefaultDenylist);

    partial void OnEagerFilesChanged(bool value) => SavePrefs(() => _prefs.FileStrategy = value ? FileClipStrategy.Eager : FileClipStrategy.Lazy);

    partial void OnHotkeysChanged(bool value)
    {
        SavePrefs(() => _prefs.HotkeysEnabled = value);
        _ = _clipboard.ApplyHotkeysAsync();
    }

    partial void OnSendToMenuChanged(bool value)
    {
        try { _sendTo.Set(value); Error = null; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Runtime.InteropServices.COMException)
        {
            Error = $"Could not change the Send to menu: {e.Message}";
        }
    }

    private void SavePrefs(Action change)
    {
        change();
        try { _prefs.Save(); Error = null; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { Error = $"Could not save: {e.Message}"; }
    }
}
