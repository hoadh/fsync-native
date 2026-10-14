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

    /// <summary>Applies retention; never removes <paramref name="keeping"/>. Returns removed folders.</summary>
    public int Clean(uint retentionDays, ulong maxBytes, IReadOnlyCollection<string> keeping, DateTime? nowUtc = null)
    {
        if (!Directory.Exists(Root)) return 0;
        var cutoff = (nowUtc ?? DateTime.UtcNow) - TimeSpan.FromDays(retentionDays);
        var folders = new List<(string Path, DateTime Modified, long Size)>();
        foreach (var d in SafeDirs(Root))
        {
            if (keeping.Contains(System.IO.Path.GetFileName(d), StringComparer.OrdinalIgnoreCase)) continue;
            folders.Add((d, Directory.GetLastWriteTimeUtc(d), SizeOf(d)));
        }
        var removed = 0;
        foreach (var f in folders.Where(f => f.Modified < cutoff).ToList())
        {
            if (TryDelete(f.Path)) { removed++; folders.Remove(f); }
        }
        var total = folders.Sum(f => f.Size) + keeping.Sum(k => SizeOf(System.IO.Path.Combine(Root, k)));
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
