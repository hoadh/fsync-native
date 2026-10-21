using Fsync.App.ViewModels;
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

    private static Style? FindStyle(string key) =>
        Application.Current.Resources.TryGetValue(key, out var value) ? value as Style : null;
}
