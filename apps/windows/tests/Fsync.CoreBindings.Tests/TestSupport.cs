using Fsync.Core;

namespace Fsync.CoreBindings.Tests;

/// <summary>Records callbacks; Rust calls these from its "fsync-callbacks" thread.</summary>
internal sealed class Recorder : EngineDelegate
{
    private readonly object _lock = new();
    private readonly List<AccountEvent> _events = [];
    private readonly int _testThread = Environment.CurrentManagedThreadId;
    public readonly ManualResetEventSlim GotAccountEvent = new();
    public bool? OnTestThread { get; private set; }

    public void OnAccountEvent(AccountEvent @event)
    {
        lock (_lock)
        {
            _events.Add(@event);
            OnTestThread = Environment.CurrentManagedThreadId == _testThread;
        }
        GotAccountEvent.Set();
    }

    public AccountEvent[] Events { get { lock (_lock) return [.. _events]; } }

    public void OnRemoteClip(ClipItem item) { }
    public void OnDeviceStatus(Device device) { }
    public void OnTransfer(TransferEvent @event) { }
    public void OnIncomingFiles(ClipItem offer) { }
    public void OnPairingEvent(PairingEvent @event) { }
    public void OnSessionChanged(SessionTokens tokens) { }
    public void OnError(FsyncException error) { }
}

/// <summary>Fresh temp data dir per test, removed afterwards.</summary>
public abstract class EngineTestBase : IDisposable
{
    private readonly List<string> _dirs = [];

    protected string TempDir()
    {
        var d = Path.Combine(Path.GetTempPath(), "fsync-cs-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(d);
        _dirs.Add(d);
        return d;
    }

    protected static EngineConfig Config(string dir, byte historyKey) => new(
        dataDir: dir,
        apiBase: "http://127.0.0.1:9",
        relayUrl: null,
        deviceName: "cs-test",
        platform: Platform.Win,
        deviceSecret: Filled(7, 32),
        historyKey: Filled(historyKey, 32),
        forceRelay: false,
        allowInsecureHttp: true);

    protected static byte[] Filled(byte value, int count)
    {
        var b = new byte[count];
        Array.Fill(b, value);
        return b;
    }

    public void Dispose()
    {
        foreach (var d in _dirs)
        {
            try { Directory.Delete(d, recursive: true); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
        GC.SuppressFinalize(this);
    }
}
