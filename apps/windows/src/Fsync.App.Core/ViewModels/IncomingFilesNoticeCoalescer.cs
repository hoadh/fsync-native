using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>
/// At most one notice per sanitized sender per <see cref="Window"/>: later offers in the window
/// return an update of that notice with a bumped count (the caller replaces the toast). UI thread only.
/// </summary>
public sealed class IncomingFilesNoticeCoalescer(TimeProvider? time = null)
{
    public static readonly TimeSpan Window = TimeSpan.FromSeconds(10);
    private readonly TimeProvider _time = time ?? TimeProvider.System;
    private readonly Dictionary<string, (DateTimeOffset Start, int Count)> _bySender = new(StringComparer.Ordinal);

    /// <summary>The notice to show (new) or to put in place of the sender's current one (<c>IsUpdate</c>).</summary>
    public IncomingFilesNotice Add(ClipItem offer, bool autoAccepted)
    {
        ArgumentNullException.ThrowIfNull(offer);
        var now = _time.GetUtcNow();
        Prune(now);
        var sender = PeerText.DisplayName(offer.fromName);
        var update = _bySender.TryGetValue(sender, out var entry);
        entry = update ? entry with { Count = entry.Count + 1 } : (now, 1);
        _bySender[sender] = entry;
        return IncomingFilesNotice.For(offer, autoAccepted, entry.Count) with { IsUpdate = update };
    }

    /// <summary>Drops expired windows, so the map stays as small as the senders of the last 10 s.</summary>
    private void Prune(DateTimeOffset now)
    {
        foreach (var key in _bySender.Where(kv => now - kv.Value.Start >= Window).Select(kv => kv.Key).ToList())
            _bySender.Remove(key);
    }
}
