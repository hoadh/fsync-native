using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>Scriptable IFsyncEngine: set the Func/Action members a test needs; records calls.</summary>
internal sealed class FakeEngine : IFsyncEngine
{
    public readonly List<string> Calls = [];
    public EngineDelegate? Delegate;

    public Func<AccountState> OnEnsure = () => AccountState.Ready;
    public Func<string, string, SessionTokens> OnLoginVerify = (_, _) => new SessionTokens("a", "r", "acct");
    public Func<Device[]> OnDevices = () => [];
    public Func<PairingMode, PairingOffer> OnOffer = _ => new PairingOffer(null, "123-456", 4_102_444_800_000);
    public Func<string, PairingPeer> OnJoin = _ => Peer(7);
    public Action<ulong, bool> OnConfirm = (_, _) => { };

    public static PairingPeer Peer(ulong id) => new(id, new string('b', 64), "Mac mini", Platform.Mac, "apple banana cherry");

    public static Device Dev(string id, string name, DeviceStatus s, bool online = false, bool self = false, ulong lastSeen = 0) =>
        new(id.PadRight(64, '0'), name, Platform.Mac, s, online, lastSeen, self);

    private T Log<T>(string call, Func<T> f) { lock (Calls) Calls.Add(call); return f(); }
    private void Log(string call) { lock (Calls) Calls.Add(call); }

    public AccountState EnsureAccount() => Log("EnsureAccount", OnEnsure);
    public SessionTokens LoginVerify(string email, string code) => Log($"LoginVerify {email} {code}", () => OnLoginVerify(email, code));
    public void LoginStart(string email) => Log($"LoginStart {email}");
    public void Logout() => Log("Logout");
    public Func<bool> OnLogoutConfirmed = () => true;
    public bool LogoutConfirmed() => Log("LogoutConfirmed", OnLogoutConfirmed);
    public void SetSession(SessionTokens tokens) => Log($"SetSession {tokens.accountId}");
    public void Start() => Log("Start");
    public void Stop() => Log("Stop");
    public string NodeId() => new('a', 64);
    public Device[] Devices() => Log("Devices", OnDevices);
    public void RemoveDevice(string nodeId) => Log($"RemoveDevice {nodeId}");
    public PairingOffer PairingOffer(PairingMode mode) => Log($"PairingOffer {mode}", () => OnOffer(mode));
    public PairingPeer PairingJoin(string input) => Log($"PairingJoin {input}", () => OnJoin(input));
    public void PairingConfirm(ulong requestId, bool accept) { Log($"PairingConfirm {requestId} {accept}"); OnConfirm(requestId, accept); }
    public void PairingCancel() => Log("PairingCancel");

    public Func<string, uint, string, ulong> OnFetchFileTo = (_, _, _) => throw new NotImplementedException();
    public readonly List<ulong> Cancelled = [];
    public void Cancel(ulong transferId) { lock (Cancelled) Cancelled.Add(transferId); Log("Cancel"); }
    public void Clear(bool keepPinned) => throw new NotImplementedException();
    public AccountState ConfirmTrustReset(string resetId) => throw new NotImplementedException();
    public void DeclineTrustReset(string resetId) => throw new NotImplementedException();
    public void Delete(string itemId) => throw new NotImplementedException();
    public ulong FetchFileTo(string itemId, uint fileId, string destDir) => Log($"FetchFileTo {fileId}", () => OnFetchFileTo(itemId, fileId, destDir));
    public byte[] FetchRep(string itemId, uint idx) => throw new NotImplementedException();
    public HistoryPage History(string? query, string? cursor, uint limit) => throw new NotImplementedException();
    public TrustResetInfo[] PendingTrustResets() => [];
    public void Pin(string itemId, bool pinned) => throw new NotImplementedException();
    public Published PublishClip(LocalClip clip) => throw new NotImplementedException();
    public byte[] ReadFileRange(string itemId, uint fileId, ulong offset, uint len) => throw new NotImplementedException();
    public AccountState ResetAccountTrust() => throw new NotImplementedException();
    public ulong SendFiles(string nodeId, string[] paths) => throw new NotImplementedException();
    public void SetSettings(Settings settings) => throw new NotImplementedException();
    public Settings Settings() => throw new NotImplementedException();
}

/// <summary>Bridge over a FakeEngine with a temp DPAPI secrets file and inline dispatch.</summary>
internal sealed class Harness : IDisposable
{
    public readonly FakeEngine Engine = new();
    public readonly string Dir = Path.Combine(Path.GetTempPath(), "fsync-app-" + Guid.NewGuid().ToString("N"));
    public readonly SecretStore Secrets;
    public readonly EngineBridge Bridge;
    public readonly List<(string Api, string Refresh, bool Insecure)> Revokes = [];
    public Action<string, string, bool> OnRevoke = (_, _, _) => { };

    public Harness(SessionTokens? session = null)
    {
        Directory.CreateDirectory(Dir);
        Secrets = new SecretStore(Path.Combine(Dir, "secrets.dat"));
        if (session is not null) Secrets.SaveSession(session, "me@example.com");
        var settings = new AppSettings(Dir, "http://127.0.0.1:9", null, "test-pc");
        Bridge = new EngineBridge(settings, Secrets, new InlineDispatcher(), (_, d) => { Engine.Delegate = d; return Engine; },
            (api, refresh, insecure) => { lock (Revokes) Revokes.Add((api, refresh, insecure)); OnRevoke(api, refresh, insecure); });
    }

    public void Dispose()
    {
        try { Directory.Delete(Dir, recursive: true); } catch (IOException) { }
    }
}
