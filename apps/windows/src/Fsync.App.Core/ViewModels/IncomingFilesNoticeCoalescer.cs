namespace Fsync.App.ViewModels;

/// <summary>
/// Rate limit for incoming-files notices (macOS IncomingFilesNoticeCoalescer): per sender, at most one
/// notice per <see cref="Window"/>. The first offer passes at once and opens a window; later offers in the
/// window are held and merged into one notice that <see cref="Flush(DateTimeOffset)"/> hands out when the
/// window ends. The caller owns the timer (<see cref="NextDeadline"/>). UI thread only.
/// </summary>
public sealed class IncomingFilesNoticeCoalescer(TimeProvider? time = null)
{
    public static readonly TimeSpan Window = TimeSpan.FromSeconds(10);
    private readonly TimeProvider _time = time ?? TimeProvider.System;

    /// <param name="Pending">Merged notice waiting for the end of the window; null when nothing was held.</param>
    private sealed record SenderWindow(DateTimeOffset Start, IncomingFilesNotice? Pending);

    private readonly Dictionary<string, SenderWindow> _senders = new(StringComparer.Ordinal);

    public IncomingFilesNotice? Submit(IncomingFilesNotice notice) => Submit(notice, _time.GetUtcNow());

    /// <summary>The notice to show now, or null when it was held for the current window.</summary>
    public IncomingFilesNotice? Submit(IncomingFilesNotice notice, DateTimeOffset now)
    {
        ArgumentNullException.ThrowIfNull(notice);
        // Idle senders cost nothing: forget those whose window is over and that hold nothing.
        foreach (var key in _senders.Where(kv => kv.Value.Pending is null && now - kv.Value.Start >= Window).Select(kv => kv.Key).ToList())
            _senders.Remove(key);
        // The key is the sanitized display name; the offer carries no better id.
        if (!_senders.TryGetValue(notice.Sender, out var sender) || now - sender.Start >= Window)
        {
            _senders[notice.Sender] = new SenderWindow(now, null);
            return notice;
        }
        // Only held offers are counted, like their files and bytes; the one already shown is not.
        var merged = sender.Pending is { } held
            ? notice with
            {
                OfferCount = held.OfferCount + 1,
                FileCount = notice.FileCount + held.FileCount,
                TotalBytes = notice.TotalBytes + held.TotalBytes,
            }
            : notice with { OfferCount = 1 };
        _senders[notice.Sender] = sender with { Pending = merged };
        return null;
    }

    public IReadOnlyList<IncomingFilesNotice> Flush() => Flush(_time.GetUtcNow());

    /// <summary>Held notices whose window has ended, oldest window first; each starts a new window for its sender.</summary>
    public IReadOnlyList<IncomingFilesNotice> Flush(DateTimeOffset now)
    {
        var due = _senders.Where(kv => kv.Value.Pending is not null && now - kv.Value.Start >= Window)
            .OrderBy(kv => kv.Value.Start).ToList();
        foreach (var (key, _) in due) _senders[key] = new SenderWindow(now, null);
        return due.Select(kv => kv.Value.Pending!).ToList();
    }

    /// <summary>When <see cref="Flush(DateTimeOffset)"/> next has something to hand out; null if nothing is held.</summary>
    public DateTimeOffset? NextDeadline =>
        _senders.Values.Where(s => s.Pending is not null).Select(s => (DateTimeOffset?)(s.Start + Window)).Min();
}
