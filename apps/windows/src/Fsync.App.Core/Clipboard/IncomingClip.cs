using Fsync.Core;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Shell;

namespace Fsync.App.Clipboard;

/// <summary>How received files reach the clipboard (ADR 010; setting, rollback switch).</summary>
public enum FileClipStrategy
{
    /// <summary>Virtual files: bytes stream from the peer when a target pastes (default).</summary>
    Lazy,
    /// <summary>Download into the inbox first, then offer CF_HDROP (targets that need real paths).</summary>
    Eager,
}

/// <summary>Turns a received <see cref="ClipItem"/> into the clipboard entry we write.</summary>
public static class IncomingClip
{
    /// <summary>Reps up to this size are fetched before writing (they ride inline: no round trip).</summary>
    public const long EagerRepBytes = 256 * 1024;

    /// <param name="fetchRep">core fetch_rep (blocking; for big reps called on the clipboard thread when a target reads).</param>
    /// <param name="readRange">core read_file_range for virtual files (blocking, any thread).</param>
    public static (List<ClipEntry> Entries, List<VirtualFile> Files) Build(
        ClipItem item, Func<uint, byte[]> fetchRep, Func<uint, long, int, byte[]> readRange)
    {
        var entries = new List<ClipEntry>();
        var seen = new HashSet<uint>();
        for (var i = 0; i < item.reps.Length; i++)
        {
            var rep = item.reps[i];
            var idx = (uint)i;
            var formats = RepresentationMapper.FormatsFor(rep.mime);
            if (formats.Count == 0) continue;
            // One fetch per rep, shared by its formats (PNG and CF_DIB).
            var bytes = new Lazy<byte[]>(() => fetchRep(idx), LazyThreadSafetyMode.ExecutionAndPublication);
            var eager = (long)rep.size <= EagerRepBytes;
            foreach (var (format, convert) in formats)
            {
                if (!seen.Add(format)) continue; // first rep of a format wins
                entries.Add(eager
                    ? ClipEntry.Eager(format, convert(bytes.Value))
                    : ClipEntry.Lazy(format, (long)rep.size, () => convert(bytes.Value)));
            }
        }
        var files = VirtualFile.WithFolders(item.files.Select(f => new VirtualFile(
            f.relPath.Length > 0 ? f.relPath : f.name,
            (long)f.size,
            DateTimeOffset.FromUnixTimeMilliseconds((long)item.tsMs).UtcDateTime,
            (offset, len) => readRange(f.fileId, offset, len))));
        return (entries, files);
    }

    /// <summary>CF_HDROP entry for files already downloaded (eager strategy).</summary>
    public static ClipEntry DropList(IEnumerable<string> paths) =>
        ClipEntry.Eager(ClipboardFormats.CF_HDROP, DropFiles.Build(paths));
}
