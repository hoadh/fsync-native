namespace Fsync.App.Files;

/// <summary>
/// Where files of received clips are downloaded (eager strategy): <c>inbox\&lt;item id&gt;\</c>
/// under the data dir (user-only ACL inherited from %LOCALAPPDATA%). Same retention as macOS:
/// folders older than the history retention go, then the oldest until under the size limit.
/// </summary>
public sealed class InboxDirectory(string root)
{
    public string Root { get; } = root;

    /// <summary>Item ids come from other devices: only a plain hex/dash id becomes a folder name.</summary>
    public static bool IsValidItemId(string id) =>
        id.Length is > 0 and <= 64 && id.All(c => c == '-' || Uri.IsHexDigit(c));

    /// <summary>Folder for one item, created on demand. Throws ArgumentException for a bad id.</summary>
    public string Folder(string itemId)
    {
        if (!IsValidItemId(itemId)) throw new ArgumentException("invalid item id", nameof(itemId));
        var dir = Path.Combine(Root, itemId);
        Directory.CreateDirectory(dir);
        return dir;
    }

    /// <summary>Folders written more recently than this are never removed (a download may be starting).</summary>
    public static readonly TimeSpan MinAge = TimeSpan.FromMinutes(5);
    private readonly object _cleanLock = new();

    /// <summary>
    /// Applies retention; never removes <paramref name="keeping"/> (clip on the clipboard, downloads
    /// in flight) nor folders younger than <see cref="MinAge"/>. One clean at a time. Returns removed folders.
    /// </summary>
    public int Clean(uint retentionDays, ulong maxBytes, IReadOnlyCollection<string> keeping, DateTime? nowUtc = null) =>
        Clean(retentionDays, maxBytes, () => keeping, nowUtc);

    /// <summary>
    /// Same, with the kept set read inside the cleanup lock, so a mark taken with
    /// <see cref="Exclusive{T}"/> is either seen by this clean or taken after it finished.
    /// </summary>
    public int Clean(uint retentionDays, ulong maxBytes, Func<IReadOnlyCollection<string>> keeping, DateTime? nowUtc = null)
    {
        lock (_cleanLock) return CleanLocked(retentionDays, maxBytes, keeping(), nowUtc ?? DateTime.UtcNow);
    }

    /// <summary>Runs <paramref name="f"/> while no clean runs (marking a folder as in use).</summary>
    public T Exclusive<T>(Func<T> f)
    {
        lock (_cleanLock) return f();
    }

    /// <summary>Marks a reused folder as recently written (cleanup minimum age).</summary>
    public void Touch(string dir)
    {
        try { Directory.SetLastWriteTimeUtc(dir, DateTime.UtcNow); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }

    private int CleanLocked(uint retentionDays, ulong maxBytes, IReadOnlyCollection<string> keeping, DateTime now)
    {
        if (!Directory.Exists(Root)) return 0;
        var cutoff = now - TimeSpan.FromDays(retentionDays);
        var folders = new List<(string Path, DateTime Modified, long Size)>();
        long protectedBytes = 0;
        foreach (var d in SafeDirs(Root))
        {
            var modified = Directory.GetLastWriteTimeUtc(d);
            if (keeping.Contains(System.IO.Path.GetFileName(d), StringComparer.OrdinalIgnoreCase) || now - modified < MinAge)
            {
                protectedBytes += SizeOf(d);
                continue;
            }
            folders.Add((d, modified, SizeOf(d)));
        }
        var removed = 0;
        foreach (var f in folders.Where(f => f.Modified < cutoff).ToList())
        {
            if (TryDelete(f.Path)) { removed++; folders.Remove(f); }
        }
        var total = folders.Sum(f => f.Size) + protectedBytes;
        foreach (var f in folders.OrderBy(f => f.Modified))
        {
            if ((ulong)Math.Max(0, total) <= maxBytes) break;
            if (TryDelete(f.Path)) { removed++; total -= f.Size; }
        }
        return removed;
    }

    private static IEnumerable<string> SafeDirs(string root)
    {
        try { return Directory.GetDirectories(root); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return []; }
    }

    internal static long SizeOf(string dir)
    {
        if (!Directory.Exists(dir)) return 0;
        try { return new DirectoryInfo(dir).EnumerateFiles("*", SearchOption.AllDirectories).Sum(f => f.Length); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return 0; }
    }

    private static bool TryDelete(string dir)
    {
        try
        {
            Directory.Delete(dir, recursive: true);
            return true;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return false; // in use (a paste still reads it): next run
        }
    }
}
