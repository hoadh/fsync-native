using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>
/// The toast for files a peer pushed (spec section 4). Never file names, clipboard content or node ids:
/// only the sanitized sender, a count and a size.
/// </summary>
/// <param name="Sender">Sanitized sender name (also the coalescing key).</param>
/// <param name="Count">Offers from this sender in the current window (1 = first notice).</param>
/// <param name="IsUpdate">True when this replaces the sender's notice already shown in the window.</param>
public sealed record IncomingFilesNotice(string Sender, string Title, string Body, int Count, bool IsUpdate = false)
{
    public static IncomingFilesNotice For(ClipItem offer, bool autoAccepted, int offerCount)
    {
        ArgumentNullException.ThrowIfNull(offer);
        var sender = PeerText.DisplayName(offer.fromName);
        var count = Math.Max(1, offerCount);
        var body = count > 1
            ? $"{count} offers from {sender}"
            : $"{Files(offer.files.Length)}, {TransferItem.Size(offer.totalSize)}. {(autoAccepted ? "Saving to Downloads" : "Open fsync to accept")}";
        return new IncomingFilesNotice(sender, $"Files from {sender}", body, count);
    }

    /// <summary>Suppressed while the Files page is visible in the active window.</summary>
    public static bool ShouldShow(bool windowActive, bool filesPageVisible) => !(windowActive && filesPageVisible);

    private static string Files(int n) => n == 1 ? "1 file" : $"{n} files";
}
