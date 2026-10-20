using Fsync.App.Files;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

/// <summary>x:Bind function helpers (no converters). Enum checks take names as string literals.</summary>
public static class Ui
{
    public static Visibility Show(bool value) => value ? Visibility.Visible : Visibility.Collapsed;
    public static Visibility ShowIf(string? value) => Show(!string.IsNullOrEmpty(value));
    public static Visibility ShowIfAny(byte[]? value) => Show(value is { Length: > 0 });

    /// <summary>Visible when <paramref name="value"/>'s name is one of the comma-separated <paramref name="names"/>.</summary>
    public static Visibility When(Enum value, string names) =>
        Show(names.Split(',', StringSplitOptions.TrimEntries).Contains(value.ToString()));

    /// <summary>
    /// Theme-aware status dot: <c>&lt;Ellipse Style="{x:Bind local:Ui.StatusDotStyle(Kind)}" /&gt;</c>.
    /// <see cref="StatusKind"/> (mapped from device state in <see cref="DeviceItem.From"/>) is the one
    /// status vocabulary; Styles/Tokens.xaml has a FsyncStatus{kind}DotStyle per value whose Fill is a
    /// {ThemeResource}, so the colour follows Light / Dark / high contrast. Plain Styles are found by a
    /// normal resource lookup (only theme dictionaries are not).
    /// </summary>
    public static Style? StatusDotStyle(StatusKind kind) => FindStyle($"FsyncStatus{kind}DotStyle");

    /// <summary>Page title (heading level 1 for screen readers) with an optional secondary line.</summary>
    public static StackPanel PageHeader(string title, string? subtitle = null)
    {
        var header = new StackPanel { Spacing = 4, Margin = new Thickness(0, 0, 0, 16) };
        var titleBlock = new TextBlock { Text = title, Style = FindStyle("FsyncPageTitleTextStyle") };
        AutomationProperties.SetHeadingLevel(titleBlock, AutomationHeadingLevel.Level1);
        header.Children.Add(titleBlock);
        if (!string.IsNullOrEmpty(subtitle))
            header.Children.Add(new TextBlock { Text = subtitle, TextWrapping = TextWrapping.Wrap, Style = FindStyle("FsyncPageSubtitleTextStyle") });
        return header;
    }

    public static string Expires(DateTimeOffset? at) => at is { } t ? $"Valid until {t.ToLocalTime():t}" : "";

    /// <summary>Segoe Fluent glyphs (spec icon map) for page headers and empty states.</summary>
    public static class Glyphs
    {
        public const string Devices = "", History = "", Files = "", Pairing = "",
            Search = "", Offline = "", Warning = "";
    }

    /// <summary>Row icon for a device, by <see cref="DeviceItem.PlatformText"/>; this PC gets its own.</summary>
    public static string PlatformGlyph(string platformText, bool isSelf) => isSelf ? "" : platformText switch
    {
        "Mac" => "",          // laptop
        "Windows" => "",      // monitor
        "Command line" => "", // command prompt
        _ => Glyphs.Devices,
    };

    /// <summary>History empty state: nothing at all, or nothing for the search.</summary>
    public static string HistoryEmptyGlyph(string? query) => string.IsNullOrWhiteSpace(query) ? Glyphs.History : Glyphs.Search;
    public static string HistoryEmptyBody(string? query) =>
        string.IsNullOrWhiteSpace(query) ? "What you copy here or on your other devices shows up here." : "Try other words.";

    /// <summary>Sender of an <see cref="IncomingOffer"/>'s item (peer-supplied, spec section 5 sanitizer).</summary>
    public static string OfferSender(ClipItem offer) => $"From {PeerText.DisplayName(offer.fromName)}";

    /// <summary>Offer contents: one (sanitized) file name or a count, plus the size.</summary>
    public static string OfferSummary(ClipItem offer)
    {
        var what = offer.files.Length == 1 ? PeerText.DisplayName(offer.files[0].name, 80) : $"{offer.files.Length} files";
        return $"{what} ({TransferItem.Size(offer.totalSize)})";
    }

    private static Style? FindStyle(string key) =>
        Application.Current.Resources.TryGetValue(key, out var value) ? value as Style : null;
}
