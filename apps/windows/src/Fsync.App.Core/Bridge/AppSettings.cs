namespace Fsync.App.Bridge;

/// <summary>
/// Where the app keeps its files and which servers it talks to. Same variables as fsync-cli:
/// FSYNC_API, FSYNC_RELAY; FSYNC_DATA_DIR overrides %LOCALAPPDATA%\fsync (a second instance
/// for tests). Plain http is allowed only for loopback backends (dev).
/// </summary>
public sealed record AppSettings(string DataDir, string ApiBase, string? RelayUrl, string DeviceName)
{
    public const string DefaultApi = "http://127.0.0.1:8080";

    public string CoreDir => Path.Combine(DataDir, "core");
    public string SecretsPath => Path.Combine(DataDir, "secrets.dat");
    public string LogPath => Path.Combine(DataDir, "app.log");

    public bool AllowInsecureHttp =>
        Uri.TryCreate(ApiBase, UriKind.Absolute, out var u) && u.Scheme == Uri.UriSchemeHttp && u.IsLoopback;

    /// <summary>Single-instance key: one app per data directory.</summary>
    public string InstanceKey => "fsync-" + Convert.ToHexString(
        System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(DataDir.ToLowerInvariant())))[..16];

    public static AppSettings FromEnvironment(Func<string, string?>? env = null)
    {
        env ??= Environment.GetEnvironmentVariable;
        var dataDir = env("FSYNC_DATA_DIR") is { Length: > 0 } d
            ? Path.GetFullPath(d)
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "fsync");
        var api = env("FSYNC_API") is { Length: > 0 } a ? a : DefaultApi;
        var relay = env("FSYNC_RELAY") is { Length: > 0 } r ? r : null;
        var name = env("FSYNC_DEVICE_NAME") is { Length: > 0 } n ? n : Environment.MachineName;
        return new AppSettings(dataDir, api, relay, name);
    }
}
