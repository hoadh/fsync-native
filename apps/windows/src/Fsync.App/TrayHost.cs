using System.ComponentModel;
using Fsync.App.Clipboard;
using Fsync.App.ViewModels;
using H.NotifyIcon;
using H.NotifyIcon.Core;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Fsync.App;

/// <summary>
/// Tray icon (H.NotifyIcon): badge color follows <see cref="TrayStatusViewModel.Kind"/>,
/// left click opens the window, right click shows the menu (clipboard actions on top).
/// </summary>
internal sealed class TrayHost : IDisposable
{
    private readonly TaskbarIcon _icon;
    private readonly TrayStatusViewModel _status;
    private readonly ClipboardService _clipboard;
    private readonly MenuFlyoutItem _activity, _pastePending;
    private StatusKind? _badge;

    public TrayHost(App app, TrayStatusViewModel status, ClipboardService clipboard)
    {
        _status = status;
        _clipboard = clipboard;
        _activity = new MenuFlyoutItem { IsEnabled = false, Visibility = Microsoft.UI.Xaml.Visibility.Collapsed };
        _pastePending = Item("Paste from another device", () => _ = clipboard.PastePendingAsync());
        var menu = new MenuFlyout();
        menu.Items.Add(_activity);
        menu.Items.Add(Item("Send clipboard now (Ctrl+Alt+C)", () => _ = clipboard.SendNowAsync()));
        menu.Items.Add(_pastePending);
        menu.Items.Add(Item("Clipboard history", () => app.ShowWindow(MainWindow.HistoryTag)));
        menu.Items.Add(Item("Files and transfers", () => app.ShowWindow(MainWindow.FilesTag)));
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(Item("Open fsync", () => app.ShowWindow()));
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
    }

    private static MenuFlyoutItem Item(string text, Action onClick)
    {
        var item = new MenuFlyoutItem { Text = text };
        item.Click += (_, _) => onClick();
        return item;
    }

    private void OnStatusChanged(object? sender, PropertyChangedEventArgs e) => Apply();

    private void Apply()
    {
        var activity = _clipboard.LastError ?? _clipboard.LastActivity;
        _icon.ToolTipText = activity is null ? _status.Tooltip : $"{_status.Tooltip}\n{activity}";
        _activity.Text = activity ?? "";
        _activity.Visibility = activity is null ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;
        _pastePending.IsEnabled = _clipboard.Pending is not null;
        _pastePending.Text = _clipboard.Pending is { } p ? $"Paste from {p.fromName} (Ctrl+Alt+V)" : "Paste from another device (Ctrl+Alt+V)";
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
        _status.Dispose();
        _icon.Dispose();
    }
}
