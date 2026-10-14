using Fsync.App.Services;
using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>History "Copy" of an item that was not received this session: full content from core.</summary>
public sealed partial class ClipboardService
{
    private Task<bool> WriteHistoryItemAsync(HistoryItem item) =>
        WriteCoreAsync(item.id, item.outgoing ? "Copied from history" : "Copied from history (received earlier)", async ct =>
        {
            // Blocking, and a rep never fetched here may need the network: off the caller thread and counted by the quit gate.
            // A newer write stops waiting for it (the call itself finishes on its own; core may skip reps it cannot obtain).
            var entries = await Task.Run(() => EntriesFor(_gate.Run(() => _bridge.Call(e => e.HistoryItemReps(item.id))))).WaitAsync(ct);
            return (entries, []);
        }, "This item has no content this PC can paste");

    /// <summary>
    /// Entries from full reps (no peer offer to fetch from). First rep of a format wins. Big reps convert lazily,
    /// like incoming ones (a PNG's CF_DIB can be many times its size); a rep that cannot be converted is skipped.
    /// </summary>
    internal static List<ClipEntry> EntriesFor(IEnumerable<Rep> reps)
    {
        var entries = new List<ClipEntry>();
        var seen = new HashSet<uint>();
        foreach (var rep in reps)
            foreach (var (format, convert) in RepresentationMapper.FormatsFor(rep.mime))
            {
                if (seen.Contains(format)) continue;
                var data = rep.data;
                try
                {
                    entries.Add(data.Length <= IncomingClip.EagerRepBytes
                        ? ClipEntry.Eager(format, convert(data))
                        : ClipEntry.Lazy(format, data.Length, () => convert(data)));
                    seen.Add(format);
                }
                catch (Exception e) when (e is InvalidDataException or ArgumentException or System.Runtime.InteropServices.COMException)
                {
                    AppLog.Warn($"clipboard: history rep {rep.mime} skipped: {e.GetType().Name}");
                }
            }
        return entries;
    }
}
