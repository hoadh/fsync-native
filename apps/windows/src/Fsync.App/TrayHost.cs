using System.ComponentModel;
using Fsync.App.ViewModels;
using H.NotifyIcon;
using H.NotifyIcon.Core;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Fsync.App;

/// <summary>
/// Tray icon (H.NotifyIcon): badge color follows <see cref="TrayStatusViewModel.Kind"/>,
/// left click opens the window, right click shows the menu.
/// </summary>
internal sealed class TrayHost : IDisposable
{
    private readonly TaskbarIcon _icon;
    private readonly TrayStatusViewModel _status;

    public TrayHost(App app, TrayStatusViewModel status)
    {
        _status = status;
        var menu = new MenuFlyout();
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
        _icon.ToolTipText = _status.Tooltip;
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
        _status.Dispose();
        _icon.Dispose();
    }
}
