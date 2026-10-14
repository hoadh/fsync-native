using Fsync.Core;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>History "Copy" of an item that was not received this session: full content from core.</summary>
public sealed partial class ClipboardService
{
    private Task<bool> WriteHistoryItemAsync(HistoryItem item) =>
        WriteCoreAsync(item.id, item.outgoing ? "Copied from history" : "Copied from history (received earlier)", async _ =>
        {
            // Blocking, and a rep never fetched here may need the network: off the caller thread, counted by the quit gate.
            var reps = await Task.Run(() => _gate.Run(() => _bridge.Call(e => e.HistoryItemReps(item.id))));
            return (await Task.Run(() => EntriesFor(reps)), []);
        });

    /// <summary>Every rep fully in memory (no lazy rendering: there is no peer offer to fetch from). First rep of a format wins.</summary>
    internal static List<ClipEntry> EntriesFor(IEnumerable<Rep> reps)
    {
        var entries = new List<ClipEntry>();
        var seen = new HashSet<uint>();
        foreach (var rep in reps)
            foreach (var (format, convert) in RepresentationMapper.FormatsFor(rep.mime))
                if (seen.Add(format)) entries.Add(ClipEntry.Eager(format, convert(rep.data)));
        return entries;
    }
}
