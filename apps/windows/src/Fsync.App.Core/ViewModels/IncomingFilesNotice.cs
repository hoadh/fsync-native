using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>
/// The toast for files a peer pushed (spec section 4, macOS IncomingFilesNotice). Never file names,
/// clipboard content or node ids: only the sanitized sender, counts and a size.
/// </summary>
/// <param name="Sender">Peer-supplied sender name; stored sanitized ("another device" when empty). Also the coalescing key.</param>
/// <param name="OfferCount">Offers folded into this notice (1 unless the coalescer merged several).</param>
public sealed record IncomingFilesNotice(string Sender, int FileCount, ulong TotalBytes, bool AutoAccepted, int OfferCount = 1)
{
    private readonly string _sender = PeerText.DisplayName(Sender);

    /// <summary>Sanitized on set; <c>with</c> copies keep the stored value as is.</summary>
    public string Sender { get => _sender; init => _sender = PeerText.DisplayName(value); }

    public static IncomingFilesNotice For(ClipItem offer, bool autoAccepted, int offerCount = 1)
    {
        ArgumentNullException.ThrowIfNull(offer);
        return new IncomingFilesNotice(offer.fromName, offer.files.Length, offer.totalSize, autoAccepted, Math.Max(1, offerCount));
    }

    public string Title => $"Files from {Sender}";

    public string Body
    {
        get
        {
            if (OfferCount > 1) return $"{OfferCount} offers from {Sender}";
            var files = FileCount == 1 ? "1 file" : $"{FileCount} files";
            return $"{files}, {TransferItem.Size(TotalBytes)}. {(AutoAccepted ? "Saving to Downloads" : "Open fsync to accept")}";
        }
    }

    /// <summary>Suppressed while the Files page is visible in the active window.</summary>
    public static bool ShouldShow(bool windowActive, bool filesPageVisible) => !(windowActive && filesPageVisible);
}
