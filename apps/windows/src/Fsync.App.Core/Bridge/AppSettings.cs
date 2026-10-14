namespace Fsync.App.Bridge;

/// <summary>
/// Where the app keeps its files and which servers it talks to. Same variables as fsync-cli:
/// FSYNC_API, FSYNC_RELAY; FSYNC_DOWNLOAD_DIR overrides Downloadssync; FSYNC_DATA_DIR overrides %LOCALAPPDATA%\fsync (a second instance
/// for tests). Plain http is allowed only for loopback backends (dev).
/// </summary>
public sealed record AppSettings(string DataDir, string ApiBase, string? RelayUrl, string DeviceName, string? DownloadDirOverride = null)
{
    public const string DefaultApi = "http://127.0.0.1:8080";

    public string CoreDir => Path.Combine(DataDir, "core");
    public string SecretsPath => Path.Combine(DataDir, "secrets.dat");
    public string LogPath => Path.Combine(DataDir, "app.log");
    public string PreferencesPath => Path.Combine(DataDir, "preferences.json");
    /// <summary>Eager file pastes land here, one folder per clip (user-only ACL, inherited from LocalAppData).</summary>
    public string InboxDir => Path.Combine(DataDir, "inbox");
    /// <summary>Accepted pushed files: Downloadssync, or FSYNC_DOWNLOAD_DIR (tests).</summary>
    public string DownloadDir => DownloadDirOverride ?? Path.Combine(Fsync.Interop.Shell.KnownFolders.DownloadsPath(), "fsync");

    public bool AllowInsecureHttp =>
        Uri.TryCreate(ApiBase, UriKind.Absolute, out var u) && u.Scheme == Uri.UriSchemeHttp && u.IsLoopback;

    /// <summary>Single-instance key: one app per data directory (any spelling of the same path).</summary>
    public string InstanceKey => "fsync-" + Convert.ToHexString(
        System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(NormalizeDir(DataDir).ToLowerInvariant())))[..16];

    /// <summary>Full path, no trailing separator, 8.3 short names expanded when the folder exists.</summary>
    public static string NormalizeDir(string dir)
    {
        var full = Path.GetFullPath(dir);
        var root = Path.GetPathRoot(full) ?? "";
        full = full.Length > root.Length ? full.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar) : full;
        if (!Directory.Exists(full)) return full;
        var buf = new System.Text.StringBuilder(1024);
        return GetLongPathName(full, buf, buf.Capacity) is > 0 and < 1024 ? buf.ToString() : full;
    }

    [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
    private static extern int GetLongPathName(string path, System.Text.StringBuilder buffer, int size);

    public static AppSettings FromEnvironment(Func<string, string?>? env = null)
    {
        env ??= Environment.GetEnvironmentVariable;
        var dataDir = env("FSYNC_DATA_DIR") is { Length: > 0 } d
            ? NormalizeDir(d)
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "fsync");
        var api = env("FSYNC_API") is { Length: > 0 } a ? a : DefaultApi;
        var relay = env("FSYNC_RELAY") is { Length: > 0 } r ? r : null;
        var name = env("FSYNC_DEVICE_NAME") is { Length: > 0 } n ? n : Environment.MachineName;
        var downloads = env("FSYNC_DOWNLOAD_DIR") is { Length: > 0 } dl ? Path.GetFullPath(dl) : null;
        return new AppSettings(dataDir, api, relay, name, downloads);
    }
}
