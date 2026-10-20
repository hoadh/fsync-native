using Fsync.App.ViewModels;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Fsync.App.Views;

/// <summary>A peer's connection state as the status dot shows it (one design token brush each).</summary>
public enum PeerStatus { Online, Lan, Relay, Offline, Attention }

/// <summary>x:Bind function helpers (no converters). Enum checks take names as string literals.</summary>
public static class Ui
{
    public static Visibility Show(bool value) => value ? Visibility.Visible : Visibility.Collapsed;
    public static Visibility ShowIf(string? value) => Show(!string.IsNullOrEmpty(value));
    public static Visibility ShowIfAny(byte[]? value) => Show(value is { Length: > 0 });

    /// <summary>Visible when <paramref name="value"/>'s name is one of the comma-separated <paramref name="names"/>.</summary>
    public static Visibility When(Enum value, string names) =>
        Show(names.Split(',', StringSplitOptions.TrimEntries).Contains(value.ToString()));

    public static Brush StatusBrush(StatusKind kind) => new SolidColorBrush(TrayHost.BadgeColor(kind));

    /// <summary>The theme-aware status brush from Styles/Tokens.xaml (FsyncStatus*Brush).</summary>
    public static Brush StatusBrush(PeerStatus status) =>
        Application.Current.Resources.TryGetValue($"FsyncStatus{status}Brush", out var value) && value is Brush brush
            ? brush
            : new SolidColorBrush(Colors.Gray);

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
