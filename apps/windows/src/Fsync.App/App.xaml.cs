using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Fsync.App;

/// <summary>
/// Composition root. Lives in the tray; the main window is created on demand and destroyed on
/// close (memory). While offline, the account check is retried every 15 s.
/// </summary>
public partial class App : Application
{
    public const uint ExpectedContract = 2;

    private readonly AppSettings _settings;
    private readonly bool _showWindowAtStart;
    private readonly IReadOnlyList<string> _startArgs;
    private DispatcherQueue? _ui;
    private DispatcherQueueTimer? _retry;
    private TrayHost? _tray;
    private MainWindow? _window;

    internal EngineBridge Bridge { get; private set; } = null!;
    internal DevicesViewModel Devices { get; private set; } = null!;
    internal PairingViewModel Pairing { get; private set; } = null!;
    internal IRunAtLogin RunAtLogin { get; private set; } = null!;
    internal AppPreferences Preferences { get; private set; } = null!;
    internal ClipboardService Clipboard { get; private set; } = null!;
    internal TransfersViewModel Transfers { get; private set; } = null!;
    internal SendToShortcut SendTo { get; private set; } = null!;
    /// <summary>Paths from Explorer "Send to", waiting for a device choice (Files page).</summary>
    internal List<string> PendingSend { get; } = [];

    public App(AppSettings settings, bool showWindowAtStart, IReadOnlyList<string> startArgs)
    {
        _settings = settings;
        _showWindowAtStart = showWindowAtStart;
        _startArgs = startArgs;
        InitializeComponent();
        // Type, HRESULT and stack only (messages may name files); the process still ends.
        UnhandledException += (_, e) => AppLog.Warn($"unhandled {e.Exception.GetType().FullName} 0x{e.Exception.HResult:X8}\n{e.Exception.StackTrace}");
    }

    internal static new App Current => (App)Application.Current;

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        _ui = DispatcherQueue.GetForCurrentThread();
        AppLog.Start(_settings.LogPath);
        var secrets = new SecretStore(_settings.SecretsPath);
        Bridge = new EngineBridge(_settings, secrets, new WinUiDispatcher(_ui));
        Devices = new DevicesViewModel(Bridge);
        Pairing = new PairingViewModel(Bridge);
        RunAtLogin = new RunAtLogin("Fsync", Services.RunAtLogin.CommandFor(Environment.ProcessPath!));
        Preferences = AppPreferences.Load(_settings.PreferencesPath);
        Clipboard = new ClipboardService(Bridge, Preferences, new WinUiDispatcher(_ui), _settings.InboxDir);
        Transfers = new TransfersViewModel(Bridge, () => _settings.DownloadDir,
            () => Clipboard.CurrentSettings?.autoAcceptFiles ?? false);
        SendTo = new SendToShortcut(Environment.ProcessPath!);
        _tray = new TrayHost(this, new TrayStatusViewModel(Bridge, Devices), Clipboard);

        // Self-check: the native DLL for this architecture loads and speaks our contract.
        try
        {
            var v = FsyncFfi.ContractVersion();
            if (v != ExpectedContract) throw new InvalidOperationException($"fsync_ffi contract {v}, app expects {ExpectedContract}");
        }
        catch (Exception e) when (e is DllNotFoundException or BadImageFormatException or EntryPointNotFoundException or InvalidOperationException)
        {
            ShowWindow();
            _window!.ShowFatal($"fsync could not load its engine: {e.Message}");
            return;
        }

        Bridge.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(EngineBridge.Phase)) OnPhaseChanged();
        };
        await Bridge.InitializeAsync();
        if (SendToArgs.Parse(_startArgs) is not null) HandleArgs(_startArgs);
        else if (_showWindowAtStart || Bridge.Phase != AccountPhase.Ready) ShowWindow();
    }

    private void OnPhaseChanged()
    {
        if (Bridge.Phase == AccountPhase.Offline) StartRetry();
        else _retry?.Stop();
        if (Bridge.Phase == AccountPhase.Ready) // idempotent; re-reads settings
            Clipboard.StartAsync().ContinueWith(t => AppLog.Warn($"clipboard start: {t.Exception?.InnerException?.GetType().Name}"), TaskContinuationOptions.OnlyOnFaulted);
        _window?.ShowPhase(Bridge.Phase);
    }

    private void StartRetry()
    {
        if (_retry is null)
        {
            _retry = _ui!.CreateTimer();
            _retry.Interval = TimeSpan.FromSeconds(15);
            _retry.Tick += async (_, _) => await Bridge.GoOnlineAsync();
        }
        _retry.Start();
    }

    /// <param name="page">Optional page to open (tray menu shortcuts).</param>
    internal void ShowWindow(string? page = null)
    {
        if (_window is null)
        {
            _window = new MainWindow();
            _window.Closed += (_, _) => _window = null;
        }
        _window.ShowPhase(Bridge?.Phase ?? AccountPhase.Starting, page);
        _window.Activate();
    }

    /// <summary>A second launch was redirected here (event arrives off the UI thread).</summary>
    internal void ShowFromAnotherInstance(IReadOnlyList<string> args) => _ui?.TryEnqueue(() => HandleArgs(args));

    /// <summary>"--send p1 p2": queue the paths and open the Files page to pick a device.</summary>
    internal void HandleArgs(IReadOnlyList<string> args)
    {
        if (SendToArgs.Parse(args) is { Count: > 0 } paths)
        {
            foreach (var p in paths) if (!PendingSend.Contains(p, StringComparer.OrdinalIgnoreCase)) PendingSend.Add(p);
            ShowWindow(MainWindow.FilesTag);
        }
        else ShowWindow();
    }

    private bool _quitting;

    /// <summary>
    /// UI parts go on the UI thread; the clipboard (≤ ~3 s) and the engine (stop, ≤ ~5 s) are
    /// torn down off it, so the message loop keeps running meanwhile; then the app exits.
    /// </summary>
    internal async void Quit()
    {
        if (_quitting) return;
        _quitting = true;
        _window?.Close();
        _tray?.Dispose();
        Devices.Dispose();
        Transfers.Dispose();
        Pairing.Dispose();
        try
        {
            await Task.Run(() =>
            {
                Clipboard.Dispose(); // exit policy: flush rendered clips, clear lazy ones (ADR 013 §5)
                Bridge.Dispose();    // stop() first
            });
        }
        catch (Exception e) { AppLog.Warn($"quit: {e.GetType().Name}"); }
        Exit();
    }
}
