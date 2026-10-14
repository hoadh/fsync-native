using Fsync.App.Bridge;
using Fsync.App.Clipboard;
using Fsync.App.Services;

namespace Fsync.App.Tests;

/// <summary>
/// The real engine (fsync_ffi.dll) in a fresh data dir, signed out and offline: settings,
/// history and transfers answer locally. Backend address is unreachable on purpose.
/// </summary>
internal sealed class RealEngine : IDisposable
{
    public string Dir { get; } = Path.Combine(Path.GetTempPath(), "fsync-app-real-" + Guid.NewGuid().ToString("N")[..8]);
    public AppSettings Settings { get; }
    public EngineBridge Bridge { get; }
    public AppPreferences Prefs { get; }
    private ClipboardService? _clipboard;

    private RealEngine()
    {
        Directory.CreateDirectory(Dir);
        Settings = new AppSettings(Dir, "http://127.0.0.1:9", null, "app-test");
        Bridge = new EngineBridge(Settings, new SecretStore(Settings.SecretsPath), new InlineDispatcher());
        Prefs = AppPreferences.Load(Settings.PreferencesPath);
    }

    public static async Task<RealEngine> StartAsync()
    {
        var r = new RealEngine();
        await r.Bridge.InitializeAsync();
        Assert.True(r.Bridge.HasEngine, r.Bridge.StatusMessage);
        Assert.Equal(AccountPhase.SignedOut, r.Bridge.Phase);
        return r;
    }

    /// <summary>A clipboard service on this engine (not listening: StartAsync is not called).</summary>
    public ClipboardService Clipboard => _clipboard ??= new ClipboardService(Bridge, Prefs, new InlineDispatcher(), Settings.InboxDir);

    public void Dispose()
    {
        _clipboard?.Dispose();
        Bridge.Dispose();
        try { Directory.Delete(Dir, recursive: true); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}
