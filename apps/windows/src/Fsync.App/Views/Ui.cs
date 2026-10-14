using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

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

    public static Brush StatusBrush(StatusKind kind) => new SolidColorBrush(TrayHost.BadgeColor(kind));

    public static string Expires(DateTimeOffset? at) => at is { } t ? $"Valid until {t.ToLocalTime():t}" : "";
}
