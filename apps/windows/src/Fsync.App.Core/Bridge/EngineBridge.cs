using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>Where the account stands, as the UI shows it.</summary>
public enum AccountPhase { Starting, SignedOut, Offline, NeedsPairing, Ready, Failed }

/// <summary>
/// The one owner of the engine. Engine calls block, so every call runs on the thread pool
/// (<see cref="RunAsync{T}"/>); results come back to the awaiting (UI) context. Core events
/// are raised on the UI thread. Observable members change on the UI thread only.
/// </summary>
public sealed partial class EngineBridge : ObservableObject, IDisposable
{
    private readonly AppSettings _settings;
    private readonly SecretStore _secrets;
    private readonly IUiDispatcher _ui;
    private readonly Func<EngineConfig, EngineDelegate, IFsyncEngine> _factory;
    private readonly Action<string, string, bool> _revoke;
    private IFsyncEngine? _engine;
    private bool _revokeResumed;

    [ObservableProperty] private AccountPhase _phase = AccountPhase.Starting;
    /// <summary>Last user-facing problem (network, core error), or null.</summary>
    [ObservableProperty] private string? _statusMessage;

    public EngineBridge(AppSettings settings, SecretStore secrets, IUiDispatcher ui,
        Func<EngineConfig, EngineDelegate, IFsyncEngine>? factory = null,
        Action<string, string, bool>? revoke = null)
    {
        _revoke = revoke ?? PendingRevokeResume.DefaultRevoke;
        _settings = settings;
        _secrets = secrets;
        _ui = ui;
        _factory = factory ?? ((cfg, d) => new FsyncEngine(cfg, d));
    }

    public string? Email => _secrets.Email;
    public string NodeId => Engine.NodeId();
    public string DeviceName => _settings.DeviceName;
    /// <summary>Set when startup failed because the history key does not match (ResetHistoryAsync).</summary>
    public bool HistoryKeyMismatch { get; private set; }

    public event Action<Device>? DeviceStatusChanged;
    /// <summary>The device list may have changed (paired, removed, list advanced).</summary>
    public event Action? DevicesChanged;
    public event Action<PairingEvent>? PairingEventReceived;
    public event Action<AccountEvent>? AccountEventReceived;

    private IFsyncEngine Engine => _engine ?? throw new InvalidOperationException("engine not created");

    /// <summary>Blocking engine call off the UI thread.</summary>
    public Task<T> RunAsync<T>(Func<IFsyncEngine, T> call) => Task.Run(() => call(Engine));
    public Task RunAsync(Action<IFsyncEngine> call) => Task.Run(() => call(Engine));

    /// <summary>Creates the engine, restores the session, starts the node, asks for the account state.</summary>
    public async Task InitializeAsync()
    {
        if (!_revokeResumed) { _revokeResumed = true; ResumePendingRevoke(); } // once per launch
        DisposeEngine(); // ResetHistoryAsync: the old engine must release its files and engine.lock first
        Directory.CreateDirectory(_settings.CoreDir);
        var cfg = new EngineConfig(_settings.CoreDir, _settings.ApiBase, null, _settings.DeviceName,
            Platform.Win, _secrets.DeviceSecret, _secrets.HistoryKey, forceRelay: false,
            allowInsecureHttp: _settings.AllowInsecureHttp, relayUrls: _settings.RelayUrls.ToArray());
        try
        {
            _engine = await Task.Run(() => _factory(cfg, new EngineDelegateAdapter(_ui, _secrets, this)));
        }
        catch (FsyncException.HistoryKeyMismatch)
        {
            HistoryKeyMismatch = true;
            Fail("The clipboard history on this PC was encrypted with a different key.");
            return;
        }
        catch (FsyncException e)
        {
            Fail($"Could not start: {UserMessages.For(e)}");
            return;
        }
        if (_secrets.Session is { } session)
        {
            await RunAsync(e => e.SetSession(session));
            await GoOnlineAsync();
        }
        else Phase = AccountPhase.SignedOut;
    }

    /// <summary>Starts the node and maps ensure_account to a phase. Safe to call again (retry).</summary>
    public async Task GoOnlineAsync()
    {
        try
        {
            var state = await RunAsync(e => { e.Start(); return e.EnsureAccount(); });
            Phase = state == AccountState.NeedsPairing ? AccountPhase.NeedsPairing : AccountPhase.Ready;
            StatusMessage = null;
            DevicesChanged?.Invoke();
        }
        catch (FsyncException.Auth)
        {
            _secrets.ClearSession();
            Phase = AccountPhase.SignedOut;
        }
        catch (FsyncException.EngineAlreadyRunning)
        {
            // Not transient: no retry loop, one clear message.
            AlreadyRunning = true;
            Fail(AlreadyRunningMessage);
        }
        catch (FsyncException e)
        {
            // Network, rate limits, a pin to another account: keep the session, allow a retry.
            Phase = AccountPhase.Offline;
            StatusMessage = UserMessages.For(e);
        }
        catch (InvalidOperationException)
        {
            // The engine is gone (quit or history reset in progress): nothing to bring online.
            AppLog.Info("bridge: go online skipped, no engine");
        }
    }

    public Task LoginStartAsync(string email) => RunAsync(e => e.LoginStart(email));

    public async Task LoginVerifyAsync(string email, string code)
    {
        var tokens = await RunAsync(e => e.LoginVerify(email, code));
        _secrets.SaveSession(tokens, email);
        await GoOnlineAsync();
    }

    /// <summary>After the user confirmed: move the old history aside and start again.</summary>
    public async Task ResetHistoryAsync()
    {
        await Task.Run(() => FsyncFfi.ResetHistory(_settings.CoreDir));
        HistoryKeyMismatch = false;
        Phase = AccountPhase.Starting;
        await InitializeAsync();
    }

    internal void HandleAccountEvent(AccountEvent ev)
    {
        switch (ev)
        {
            case AccountEvent.SessionExpired:
                _secrets.ClearSession();
                Phase = AccountPhase.SignedOut;
                break;
            case AccountEvent.SelfRemoved:
                Phase = AccountPhase.NeedsPairing;
                DevicesChanged?.Invoke();
                break;
            case AccountEvent.ListAdvanced:
                DevicesChanged?.Invoke();
                break;
            case AccountEvent.HistoryReset:
                StatusMessage = "The clipboard history was damaged and has been reset.";
                break;
        }
        AccountEventReceived?.Invoke(ev);
    }

    internal void HandlePairingEvent(PairingEvent ev)
    {
        PairingEventReceived?.Invoke(ev);
        // A completed pairing changes the device list and, for a new device, the account state.
        if (ev is PairingEvent.Completed) _ = GoOnlineAsync();
    }

    internal void RaiseDeviceStatus(Device d) => DeviceStatusChanged?.Invoke(d);
    internal void ReportError(FsyncException e) => StatusMessage = UserMessages.For(e);

    private void Fail(string message)
    {
        Phase = AccountPhase.Failed;
        StatusMessage = message;
    }

    public void Dispose() => DisposeEngine();

    private void DisposeEngine()
    {
        if (_engine is null) return;
        try { _engine.Stop(); } catch (FsyncException) { }
        (_engine as IDisposable)?.Dispose();
        _engine = null;
    }
}
