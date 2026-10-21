using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Views.Controls;

/// <summary>
/// x:Bind function helpers for the page restyle (phase 10): spec icon map glyphs, empty-state copy and
/// display text for peer-supplied names (spec section 5 sanitizer, "another device" when empty).
/// </summary>
public static class PageText
{
    /// <summary>Segoe Fluent glyphs (spec icon map).</summary>
    public static class Glyphs
    {
        public const string Devices = "", History = "", Files = "", Pairing = "",
            Search = "", Offline = "", Warning = "";
    }

    /// <summary>A peer-supplied name as visible text (never for UIA-locked names such as DeviceItem.ToString()).</summary>
    public static string PeerName(string? name) => PeerText.DisplayName(name);

    /// <summary>Row icon for a device, by <c>DeviceItem.PlatformText</c>; this PC gets its own.</summary>
    public static string PlatformGlyph(string platformText, bool isSelf) => isSelf ? "" : platformText switch
    {
        "Mac" => "",          // laptop
        "Windows" => "",      // monitor
        "Command line" => "", // command prompt
        _ => Glyphs.Devices,
    };

    /// <summary>History empty state: nothing at all, or nothing for the search.</summary>
    public static string HistoryEmptyGlyph(string? query) => IsSearch(query) ? Glyphs.Search : Glyphs.History;
    public static string HistoryEmptyTitle(string? query) => IsSearch(query) ? "No matches." : "Nothing copied yet.";
    public static string HistoryEmptyBody(string? query) =>
        IsSearch(query) ? "Try other words." : "What you copy here or on your other devices shows up here.";

    /// <summary>Sender of an <see cref="IncomingOffer"/>'s item.</summary>
    public static string OfferSender(ClipItem offer) => $"From {PeerName(offer.fromName)}";

    /// <summary>Offer contents: one (sanitized) file name or a count, plus the size.</summary>
    public static string OfferSummary(ClipItem offer)
    {
        var what = offer.files.Length == 1 ? PeerText.DisplayName(offer.files[0].name, 80) : $"{offer.files.Length} files";
        return $"{what} ({TransferItem.Size(offer.totalSize)})";
    }

    private static bool IsSearch(string? query) => !string.IsNullOrWhiteSpace(query);
}
