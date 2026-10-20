using System.Collections.Specialized;
using System.ComponentModel;
using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.ViewModels;
using H.NotifyIcon;
using H.NotifyIcon.Core;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Fsync.App;

/// <summary>
/// Tray icon (H.NotifyIcon): badge color follows <see cref="TrayStatusViewModel.Kind"/>,
/// left click opens the window, right click shows the menu (clipboard actions on top). Pending file
/// offers show as "Files (n)" and in the tooltip (the only signal when toasts are unavailable).
/// </summary>
internal sealed class TrayHost : IDisposable
{
    private readonly TaskbarIcon _icon;
    private readonly TrayStatusViewModel _status;
    private readonly ClipboardService _clipboard;
    private readonly TransfersViewModel _transfers;
    private readonly MenuFlyoutItem _activity, _pastePending, _files;
    private StatusKind? _badge;

    public TrayHost(App app, TrayStatusViewModel status, ClipboardService clipboard, TransfersViewModel transfers)
    {
        _status = status;
        _clipboard = clipboard;
        _transfers = transfers;
        _activity = new MenuFlyoutItem { IsEnabled = false, Visibility = Microsoft.UI.Xaml.Visibility.Collapsed };
        _pastePending = Item("Paste from another device", () => _ = clipboard.PastePendingAsync());
        _files = Item("Files", () => app.ShowWindow(MainWindow.FilesTag));
        var menu = new MenuFlyout();
        menu.Items.Add(_activity);
        menu.Items.Add(Item($"Send clipboard now ({ClipboardService.SendHotKeyText})", () => _ = clipboard.SendNowAsync()));
        menu.Items.Add(_pastePending);
        menu.Items.Add(Item("Clipboard history", () => app.ShowWindow(MainWindow.HistoryTag)));
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(Item("Open fsync", () => app.ShowWindow()));
        menu.Items.Add(_files);
        menu.Items.Add(Item("Pair a device...", () => app.ShowWindow(MainWindow.PairingTag)));
        menu.Items.Add(Item("Settings", () => app.ShowWindow(MainWindow.SettingsTag)));
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(Item("Quit fsync", app.Quit));

        _icon = new TaskbarIcon
        {
            ContextFlyout = menu,
            ContextMenuMode = ContextMenuMode.SecondWindow,
            NoLeftClickDelay = true,
            LeftClickCommand = new CommunityToolkit.Mvvm.Input.RelayCommand(() => app.ShowWindow()),
        };
        Apply();
        _icon.ForceCreate(enablesEfficiencyMode: false);
        _status.PropertyChanged += OnStatusChanged;
        _clipboard.PropertyChanged += OnStatusChanged;
        _transfers.Offers.CollectionChanged += OnOffersChanged;
    }

    private static MenuFlyoutItem Item(string text, Action onClick)
    {
        var item = new MenuFlyoutItem { Text = text };
        item.Click += (_, _) => onClick();
        return item;
    }

    private void OnStatusChanged(object? sender, PropertyChangedEventArgs e) => Apply();

    private void OnOffersChanged(object? sender, NotifyCollectionChangedEventArgs e) => Apply();

    /// <summary>"n incoming file offers": the signal for offers when toasts are off or were missed.</summary>
    internal static string? OffersText(int count) => count switch
    {
        <= 0 => null,
        1 => "1 incoming file offer",
        _ => $"{count} incoming file offers",
    };

    /// <summary>NOTIFYICONDATA.szTip holds 128 UTF-16 units including the terminator.</summary>
    internal const int TooltipLimit = 127;

    /// <summary>
    /// Cuts <paramref name="text"/> to <see cref="TooltipLimit"/> UTF-16 units on a text-element boundary
    /// (no split surrogate pair or grapheme), ending with "…" when cut.
    /// </summary>
    internal static string FitTooltip(string text)
    {
        if (text.Length <= TooltipLimit) return text;
        var info = new System.Globalization.StringInfo(text);
        var sb = new System.Text.StringBuilder(TooltipLimit);
        for (var i = 0; i < info.LengthInTextElements; i++)
        {
            var element = info.SubstringByTextElements(i, 1);
            if (sb.Length + element.Length > TooltipLimit - 1) break; // room for the ellipsis
            sb.Append(element);
        }
        return sb.ToString().TrimEnd() + "…";
    }

    private void Apply()
    {
        var activity = _clipboard.LastError ?? _clipboard.LastActivity;
        var offers = _transfers.Offers.Count;
        var tooltip = OffersText(offers) is { } o ? $"{_status.Tooltip}\n{o}" : _status.Tooltip;
        _icon.ToolTipText = FitTooltip(activity is null ? tooltip : $"{tooltip}\n{activity}");
        _files.Text = offers > 0 ? $"Files ({offers})" : "Files";
        _activity.Text = activity ?? "";
        _activity.Visibility = activity is null ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;
        _pastePending.IsEnabled = _clipboard.Pending is not null;
        _pastePending.Text = _clipboard.Pending is { } p ? $"Paste from {p.fromName} ({ClipboardService.PasteHotKeyText})" : $"Paste from another device ({ClipboardService.PasteHotKeyText})";
        // Regenerate the icon only when the badge changes: clipboard activity calls this often
        // and each GeneratedIconSource renders a new bitmap.
        if (_badge == _status.Kind) return;
        _badge = _status.Kind;
        _icon.IconSource = new GeneratedIconSource
        {
            Text = "f",
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            Foreground = new SolidColorBrush(Color.FromArgb(255, 255, 255, 255)),
            Background = new SolidColorBrush(BadgeColor(_status.Kind)),
        };
    }

    internal static Color BadgeColor(StatusKind kind) => kind switch
    {
        StatusKind.Good => Color.FromArgb(255, 16, 124, 16),     // green
        StatusKind.Warning => Color.FromArgb(255, 202, 80, 16),  // orange
        StatusKind.Bad => Color.FromArgb(255, 164, 38, 44),      // red
        _ => Color.FromArgb(255, 96, 94, 92),                    // grey
    };

    public void Dispose()
    {
        _status.PropertyChanged -= OnStatusChanged;
        _clipboard.PropertyChanged -= OnStatusChanged;
        _transfers.Offers.CollectionChanged -= OnOffersChanged;
        _status.Dispose();
        _icon.Dispose();
    }
}
