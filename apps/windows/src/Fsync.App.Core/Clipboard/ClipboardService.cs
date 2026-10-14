using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>
/// Clipboard sync. Out: WM_CLIPBOARDUPDATE → (150 ms settle) → skip rules + denylist → mapper →
/// publish_clip, one at a time, newest wins (auto mode; manual mode only on <see cref="SendNowAsync"/>
/// / Win+Alt+C). In: on_remote_clip → write, one at a time, newest wins (auto) or keep as
/// <see cref="Pending"/> (manual; Win+Alt+V). The listener and our clipboard entries live on a
/// dedicated STA thread, so delayed renders never block the UI. Observable members change on the
/// UI thread. Every engine call made for the clipboard goes through <see cref="_gate"/> (quit).
/// </summary>
public sealed partial class ClipboardService : ObservableObject, IDisposable
{
    public const int SendHotKeyId = 1, PasteHotKeyId = 2;
    /// <summary>Win+Alt: Ctrl+Alt is AltGr on many layouts (AltGr+C types "ć" in Polish).</summary>
    public const HotKeyModifiers HotKeyMods = HotKeyModifiers.Win | HotKeyModifiers.Alt;
    public const string SendHotKeyText = "Win+Alt+C", PasteHotKeyText = "Win+Alt+V";
    private const uint VK_C = 0x43, VK_V = 0x56;
    private static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(150);

    private readonly EngineBridge _bridge;
    private readonly AppPreferences _prefs;
    private readonly IUiDispatcher _ui;
    private readonly InboxDirectory _inbox;
    private readonly StaThread _sta = new("fsync-clipboard");
    private readonly EngineCallGate _gate = new();
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
        _inbox = new InboxDirectory(inboxDir);
        _settle = new Timer(_ => _sta.Post(ReadAndPublishIfAuto));
        _cleanup = new Timer(_ => CleanInbox());
        _bridge.RemoteClip += OnRemoteClip;
        _bridge.Transfer += OnTransfer;
    }

    public bool IsManual => _settings?.clipMode == ClipMode.Manual;
    /// <summary>Core settings as last read (null before the engine answered).</summary>
    public Settings? CurrentSettings => _settings;
    public InboxDirectory Inbox => _inbox;

    /// <summary>Starts listening (call once the engine exists). Safe to call again after settings change.</summary>
    public async Task StartAsync()
    {
        await RefreshSettingsAsync();
        try
        {
            await _sta.InvokeAsync(() =>
            {
                if (_listener is not null) return;
                _listener = new ClipboardListener();
                _listener.Changed += _ => _settle.Change(Settle, Timeout.InfiniteTimeSpan);
                _listener.HotKey += OnHotKey;
            });
        }
        catch (Exception e) when (e is InvalidOperationException or ObjectDisposedException)
        {
            AppLog.Warn($"clipboard: listener not started: {e.GetType().Name}");
            LastError = "The clipboard cannot be watched right now.";
            return;
        }
        await ApplyHotkeysAsync();
        _cleanup.Change(TimeSpan.Zero, CleanupInterval);
        AppLog.Info($"clipboard: listening, mode {_settings?.clipMode}, hotkeys {HotkeysRegistered}");
    }

    /// <summary>Re-reads core settings (mode, size limit, denylist, retention) after they changed.</summary>
    public async Task RefreshSettingsAsync()
    {
        try
        {
            _settings = await _bridge.RunAsync(e => e.Settings());
            if (_listener is not null) _cleanup.Change(TimeSpan.Zero, CleanupInterval); // retention may have changed
        }
        catch (FsyncException e)
        {
            LastError = UserMessages.For(e);
            AppLog.Warn($"clipboard: settings unavailable: {e.GetType().Name}");
        }
    }

    public async Task ApplyHotkeysAsync()
    {
        bool ok;
        try
        {
            ok = await _sta.InvokeAsync(() =>
            {
                if (_listener is null) return false;
                _listener.UnregisterHotKey(SendHotKeyId);
                _listener.UnregisterHotKey(PasteHotKeyId);
                if (!_prefs.HotkeysEnabled) return false;
                return _listener.RegisterHotKey(SendHotKeyId, HotKeyMods, VK_C) & _listener.RegisterHotKey(PasteHotKeyId, HotKeyMods, VK_V);
            });
        }
        catch (ObjectDisposedException) { return; }
        HotkeysRegistered = ok;
        if (_prefs.HotkeysEnabled && !ok) LastError = $"{SendHotKeyText} / {PasteHotKeyText} are used by another app.";
    }

    private void OnHotKey(int id)
    {
        if (id == SendHotKeyId) _ui.Post(() => Observe(SendNowAsync(), "send now"));
        else if (id == PasteHotKeyId) _ui.Post(() => Observe(PastePendingAsync(), "paste pending"));
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

    /// <summary>Fire-and-forget with every failure logged (no unobserved task exceptions).</summary>
    private static void Observe(Task t, string what) =>
        t.ContinueWith(x => AppLog.Warn($"clipboard: {what} failed: {x.Exception?.InnerException?.GetType().Name}"),
            CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted, TaskScheduler.Default);
}
