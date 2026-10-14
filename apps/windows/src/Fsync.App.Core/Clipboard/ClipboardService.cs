using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>
/// Clipboard sync. Out: WM_CLIPBOARDUPDATE → (150 ms settle) → skip rules + denylist → mapper →
/// publish_clip (auto mode; manual mode only on <see cref="SendNowAsync"/> / Ctrl+Alt+C).
/// In: on_remote_clip → write (auto) or keep as <see cref="Pending"/> (manual; Ctrl+Alt+V).
/// The listener and our clipboard entries live on a dedicated STA thread, so delayed renders
/// (network fetches) never block the UI. Observable members change on the UI thread.
/// </summary>
public sealed partial class ClipboardService : ObservableObject, IDisposable
{
    public const int SendHotKeyId = 1, PasteHotKeyId = 2;
    private const uint VK_C = 0x43, VK_V = 0x56;
    private static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(150);

    private readonly EngineBridge _bridge;
    private readonly AppPreferences _prefs;
    private readonly IUiDispatcher _ui;
    private readonly string _inboxDir;
    private readonly StaThread _sta = new("fsync-clipboard");
    private readonly Timer _settle;
    private ClipboardListener? _listener;
    private volatile Settings? _settings;

    /// <summary>Last thing that happened ("Sent to 2 devices", ...), names only.</summary>
    [ObservableProperty] private string? _lastActivity;
    [ObservableProperty] private string? _lastError;
    /// <summary>Manual mode: the newest received clip, not yet written to the clipboard.</summary>
    [ObservableProperty] private ClipItem? _pending;
    [ObservableProperty] private bool _hotkeysRegistered;

    public ClipboardService(EngineBridge bridge, AppPreferences prefs, IUiDispatcher ui, string inboxDir)
    {
        _bridge = bridge;
        _prefs = prefs;
        _ui = ui;
        _inboxDir = inboxDir;
        _settle = new Timer(_ => _sta.Post(ReadAndPublishIfAuto));
        _bridge.RemoteClip += OnRemoteClip;
        _bridge.Transfer += OnTransfer;
    }

    public bool IsManual => _settings?.clipMode == ClipMode.Manual;

    /// <summary>Starts listening (call once the engine exists). Safe to call again after settings change.</summary>
    public async Task StartAsync()
    {
        await RefreshSettingsAsync();
        await _sta.InvokeAsync(() =>
        {
            if (_listener is not null) return;
            _listener = new ClipboardListener();
            _listener.Changed += _ => _settle.Change(Settle, Timeout.InfiniteTimeSpan);
            _listener.HotKey += OnHotKey;
        });
        await ApplyHotkeysAsync();
    }

    /// <summary>Re-reads core settings (mode, size limit, denylist) after they changed.</summary>
    public async Task RefreshSettingsAsync()
    {
        try { _settings = await _bridge.RunAsync(e => e.Settings()); }
        catch (FsyncException e) { LastError = UserMessages.For(e); }
    }

    public async Task ApplyHotkeysAsync()
    {
        var ok = await _sta.InvokeAsync(() =>
        {
            if (_listener is null) return false;
            _listener.UnregisterHotKey(SendHotKeyId);
            _listener.UnregisterHotKey(PasteHotKeyId);
            if (!_prefs.HotkeysEnabled) return false;
            var mods = HotKeyModifiers.Control | HotKeyModifiers.Alt;
            return _listener.RegisterHotKey(SendHotKeyId, mods, VK_C) & _listener.RegisterHotKey(PasteHotKeyId, mods, VK_V);
        });
        HotkeysRegistered = ok;
        if (_prefs.HotkeysEnabled && !ok) LastError = "Ctrl+Alt+C / Ctrl+Alt+V are used by another app.";
    }

    private void OnHotKey(int id)
    {
        if (id == SendHotKeyId) _ui.Post(() => _ = SendNowAsync());
        else if (id == PasteHotKeyId) _ui.Post(() => _ = PastePendingAsync());
    }

    /// <summary>Publishes what is on the clipboard now, in any mode (menu, hotkey).</summary>
    public async Task SendNowAsync()
    {
        var snap = await _sta.InvokeAsync(ReadSnapshot);
        if (snap.Skip != SkipReason.None)
        {
            Post(() => LastActivity = snap.Skip is SkipReason.NoContent ? "Nothing to send" : $"Not sent ({Describe(snap.Skip)})");
            return;
        }
        await PublishAsync(snap);
    }

    private ClipboardSnapshot ReadSnapshot()
    {
        var s = _settings;
        return ClipboardReader.Read((long)(s?.maxClipBytes ?? 64 * 1024 * 1024), s?.denylist ?? SensitiveFormatFilter.DefaultDenylist);
    }

    private void ReadAndPublishIfAuto()
    {
        if (IsManual || _bridge.Phase != AccountPhase.Ready || _settings is null) return;
        var snap = ReadSnapshot();
        if (snap.Skip == SkipReason.None) _ = PublishAsync(snap);
        else System.Diagnostics.Trace.TraceInformation($"clipboard change skipped: {snap.Describe()}");
    }

    private async Task PublishAsync(ClipboardSnapshot snap)
    {
        try
        {
            var clip = await RepresentationMapper.ToLocalClipAsync(snap);
            if (clip is null) return;
            var r = await _bridge.RunAsync(e => e.PublishClip(clip));
            if (r.echo) return; // the clip we just received: core's echo guard
            Post(() =>
            {
                LastActivity = r.delivered.Length == 0 ? "Copied, but no other device is connected"
                    : $"Sent to {r.delivered.Length} device{(r.delivered.Length == 1 ? "" : "s")}";
                LastError = r.failed.Length > 0 ? $"{r.failed.Length} device(s) did not get it" : null;
            });
        }
        catch (FsyncException e) { Post(() => LastError = UserMessages.For(e)); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
            Post(() => LastError = $"Could not send: {e.Message}");
        }
    }

    internal static string Describe(SkipReason r) => r switch
    {
        SkipReason.DeniedApp => "copied from an app on the privacy list",
        SkipReason.TooLarge => "larger than the size limit",
        SkipReason.Unreadable => "the clipboard is busy",
        SkipReason.OwnEcho => "it came from fsync",
        _ => "marked private by the app that copied it",
    };

    private void Post(Action a) => _ui.Post(a);

    public void Dispose()
    {
        _bridge.RemoteClip -= OnRemoteClip;
        _bridge.Transfer -= OnTransfer;
        _settle.Dispose();
        try { _sta.Invoke(ReleaseClipboard); }
        catch (Exception e) when (e is System.Runtime.InteropServices.COMException or InvalidOperationException)
        {
            System.Diagnostics.Trace.TraceWarning($"clipboard release on exit: {e.Message}");
        }
        _sta.Dispose();
    }
}
