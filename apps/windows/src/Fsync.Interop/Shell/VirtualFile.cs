namespace Fsync.Interop.Shell;

/// <summary>Reads up to <c>len</c> bytes at <c>offset</c> (core: <c>read_file_range</c>, blocking).</summary>
public delegate byte[] RangeReader(long offset, int len);

/// <summary>
/// One entry of a lazy file paste: metadata now, bytes via <see cref="Reader"/> when a target
/// pastes. <see cref="RelativePath"/> uses '\' and may name sub folders; folders have no reader.
/// </summary>
public sealed record VirtualFile(string RelativePath, long Length, DateTime Modified, RangeReader? Reader, bool IsDirectory = false)
{
    public static VirtualFile Directory(string relativePath) => new(relativePath, 0, DateTime.UtcNow, null, true);

    /// <summary>
    /// Adds a folder entry before the first file inside it (the shell creates folders from those),
    /// converts '/' to '\' and drops entries whose path tries to leave the paste target.
    /// </summary>
    public static List<VirtualFile> WithFolders(IEnumerable<VirtualFile> files)
    {
        var result = new List<VirtualFile>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var f in files)
        {
            var path = Normalize(f.RelativePath);
            if (path is null) continue;
            var parts = path.Split('\\');
            for (var i = 1; i < parts.Length; i++)
            {
                var dir = string.Join('\\', parts[..i]);
                if (seen.Add(dir)) result.Add(Directory(dir));
            }
            if (seen.Add(path)) result.Add(f with { RelativePath = path });
        }
        return result;
    }

    /// <summary>Safe relative path or null (absolute, "..", empty or invalid names).</summary>
    public static string? Normalize(string rel)
    {
        var parts = rel.Replace('/', '\\').Split('\\', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0) return null;
        var invalid = Path.GetInvalidFileNameChars();
        foreach (var p in parts)
            if (p is "." or ".." || p.IndexOfAny(invalid) >= 0 || p.EndsWith('.') || p.EndsWith(' ')) return null;
        var path = string.Join('\\', parts);
        return path.Length <= 259 ? path : null; // FILEDESCRIPTORW.cFileName is MAX_PATH
    }
}
