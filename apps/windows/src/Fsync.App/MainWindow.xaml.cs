using Fsync.App.Bridge;
using Fsync.App.Views;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace Fsync.App;

/// <summary>
/// Signed out: onboarding only. Needs pairing: the pairing page only. Ready: devices, pairing,
/// settings. Starting / failed: an overlay (with the history-reset action on a key mismatch).
/// </summary>
public sealed partial class MainWindow : Window
{
    public const string DevicesTag = "devices", HistoryTag = "history", FilesTag = "files", PairingTag = "pairing", SettingsTag = "settings";
    private bool _fatal;

    public MainWindow()
    {
        InitializeComponent();
        AppWindow.Resize(new SizeInt32(720, 680)); // wide enough for the five top-level tabs (no "More" overflow)
        if (AppWindow.Presenter is OverlappedPresenter p) p.IsMaximizable = false;
    }

    internal void ShowPhase(AccountPhase phase, string? page = null)
    {
        if (_fatal) return;
        var app = App.Current;
        Overlay.Visibility = phase is AccountPhase.Starting or AccountPhase.Failed ? Visibility.Visible : Visibility.Collapsed;
        Spinner.IsActive = phase == AccountPhase.Starting;
        OverlayText.Text = phase == AccountPhase.Failed ? app.Bridge.StatusMessage ?? "fsync could not start." : "Starting...";
        ResetHistoryButton.Visibility = phase == AccountPhase.Failed && app.Bridge.HistoryKeyMismatch ? Visibility.Visible : Visibility.Collapsed;

        var ready = phase is AccountPhase.Ready or AccountPhase.Offline;
        foreach (var item in Nav.MenuItems.OfType<NavigationViewItem>())
            item.IsEnabled = ready || (string)item.Tag == PairingTag && phase == AccountPhase.NeedsPairing;
        Nav.IsPaneVisible = ready;

        switch (phase)
        {
            case AccountPhase.SignedOut: Navigate(typeof(OnboardingPage)); break;
            case AccountPhase.NeedsPairing: Navigate(typeof(PairingPage)); break;
            case AccountPhase.Ready or AccountPhase.Offline:
                Select(page ?? (ContentFrame.Content is DevicesPage or HistoryPage or TransfersPage or PairingPage or SettingsPage ? null : DevicesTag));
                if (page == FilesTag && ContentFrame.Content is TransfersPage files) _ = files.ShowSendAsync(); // already open
                break;
        }
    }

    internal void ShowFatal(string message)
    {
        _fatal = true;
        Overlay.Visibility = Visibility.Visible;
        Spinner.IsActive = false;
        OverlayText.Text = message;
    }

    private void Select(string? tag)
    {
        if (tag is null) return;
        Nav.SelectedItem = Nav.MenuItems.OfType<NavigationViewItem>().First(i => (string)i.Tag == tag);
    }

    private void OnNavSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItem is NavigationViewItem { Tag: string tag })
            Navigate(tag switch
            {
                HistoryTag => typeof(HistoryPage),
                FilesTag => typeof(TransfersPage),
                PairingTag => typeof(PairingPage),
                SettingsTag => typeof(SettingsPage),
                _ => typeof(DevicesPage),
            });
    }

    private void Navigate(Type page)
    {
        if (ContentFrame.Content?.GetType() != page) ContentFrame.Navigate(page);
    }

    private async void OnResetHistory(object sender, RoutedEventArgs e)
    {
        var confirm = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "Start a new clipboard history?",
            Content = "The old history is kept aside but can no longer be opened. This PC will need to be paired again.",
            PrimaryButtonText = "Start new history",
            CloseButtonText = "Cancel",
        };
        if (await Views.Dialogs.ShowAsync(confirm) == ContentDialogResult.Primary) await App.Current.Bridge.ResetHistoryAsync();
    }
}
