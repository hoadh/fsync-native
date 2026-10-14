using Fsync.App.Bridge;
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
    private DispatcherQueue? _ui;
    private DispatcherQueueTimer? _retry;
    private TrayHost? _tray;
    private MainWindow? _window;

    internal EngineBridge Bridge { get; private set; } = null!;
    internal DevicesViewModel Devices { get; private set; } = null!;
    internal PairingViewModel Pairing { get; private set; } = null!;
    internal IRunAtLogin RunAtLogin { get; private set; } = null!;

    public App(AppSettings settings, bool showWindowAtStart)
    {
        _settings = settings;
        _showWindowAtStart = showWindowAtStart;
        InitializeComponent();
    }

    internal static new App Current => (App)Application.Current;

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        _ui = DispatcherQueue.GetForCurrentThread();
        var secrets = new SecretStore(_settings.SecretsPath);
        Bridge = new EngineBridge(_settings, secrets, new WinUiDispatcher(_ui));
        Devices = new DevicesViewModel(Bridge);
        Pairing = new PairingViewModel(Bridge);
        RunAtLogin = new RunAtLogin("Fsync", Services.RunAtLogin.CommandFor(Environment.ProcessPath!));
        _tray = new TrayHost(this, new TrayStatusViewModel(Bridge, Devices));

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
        if (_showWindowAtStart || Bridge.Phase != AccountPhase.Ready) ShowWindow();
    }

    private void OnPhaseChanged()
    {
        if (Bridge.Phase == AccountPhase.Offline) StartRetry();
        else _retry?.Stop();
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
    internal void ShowFromAnotherInstance() => _ui?.TryEnqueue(() => ShowWindow());

    internal void Quit()
    {
        _window?.Close();
        _tray?.Dispose();
        Devices.Dispose();
        Pairing.Dispose();
        Bridge.Dispose(); // stop() first; blocks up to ~5 s
        Exit();
    }
}
