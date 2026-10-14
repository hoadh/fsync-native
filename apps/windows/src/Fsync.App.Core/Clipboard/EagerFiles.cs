using Fsync.Core;
using Fsync.Interop.Shell;
using CoreFile = Fsync.Core.FileInfo;

namespace Fsync.App.Clipboard;

/// <summary>
/// The eager file strategy (ADR 010: a fallback; lazy virtual files are the default). Only clips
/// up to <see cref="Cap"/> in total are downloaded before writing; larger ones stay lazy.
/// Downloads keep the folder layout (core places files at dest\rel_path), so CF_HDROP lists the
/// top-level entries, and a folder pastes as a folder.
/// </summary>
public static class EagerFiles
{
    public const long Cap = 20L * 1024 * 1024;
    /// <summary>How long a paste may wait for its downloads (macOS: 180 s).</summary>
    public static readonly TimeSpan Timeout = TimeSpan.FromSeconds(180);

    public static bool Applies(FileClipStrategy strategy, ClipItem item) =>
        strategy == FileClipStrategy.Eager && item.files.Length > 0 && item.files.Sum(f => (long)f.size) <= Cap;

    /// <summary>Relative path of a received file, or null when it is unsafe.</summary>
    public static string? RelPath(CoreFile f) => VirtualFile.Normalize(f.relPath.Length > 0 ? f.relPath : f.name);

    /// <summary>A complete earlier download (copy-back of the same clip): reused, never "name (1)".</summary>
    public static string? Existing(string dir, CoreFile f)
    {
        if (RelPath(f) is not { } rel) return null;
        var path = Path.Combine(dir, rel);
        try { return File.Exists(path) && new System.IO.FileInfo(path).Length == (long)f.size ? path : null; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; }
    }

    /// <summary>Top-level entries (files and folders) to put in CF_HDROP.</summary>
    public static List<string> TopLevel(string dir, IEnumerable<CoreFile> files)
    {
        var tops = new List<string>();
        foreach (var f in files)
        {
            if (RelPath(f) is not { } rel) continue;
            var top = Path.Combine(dir, rel.Split('\\')[0]);
            if (!tops.Contains(top, StringComparer.OrdinalIgnoreCase)) tops.Add(top);
        }
        return tops;
    }
}
