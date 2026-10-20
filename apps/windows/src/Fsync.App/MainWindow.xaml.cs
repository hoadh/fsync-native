using Fsync.App.Bridge;
using Fsync.App.Shell;
using Fsync.App.Views;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Windows.Graphics;

namespace Fsync.App;

/// <summary>
/// Signed out: onboarding only. Needs pairing: the pairing page only. Ready: devices, history,
/// files, pairing and (footer) settings. Starting / failed: an overlay (with the history-reset
/// action on a key mismatch). Recreated on every open from the tray; view models live on
/// <see cref="App"/>, so anything subscribed here is unsubscribed in <see cref="OnClosed"/>.
/// </summary>
public sealed partial class MainWindow : Window
{
    public const string DevicesTag = NavigationRoutes.DevicesTag, HistoryTag = NavigationRoutes.HistoryTag,
        FilesTag = NavigationRoutes.FilesTag, PairingTag = NavigationRoutes.PairingTag, SettingsTag = NavigationRoutes.SettingsTag;
    private bool _fatal;
    private bool _navLoaded;
    private bool _ready;
    /// <summary>A Select(SettingsTag) before Nav.Loaded (the footer item does not exist yet), replayed in Loaded.</summary>
    private string? _pendingTag;

    public MainWindow()
    {
        InitializeComponent();
        AppWindow.Resize(new SizeInt32(900, 680));
        // No minimum size: WASDK 1.6 OverlappedPresenter has no PreferredMinimumWidth/Height (added in
        // a later WASDK). Follow-up: a WM_GETMINMAXINFO subclass, or the presenter API after an upgrade.
        if (AppWindow.Presenter is OverlappedPresenter p) p.IsMaximizable = false;
        TitleBarSetup.Apply(this, AppTitleBar);
        if (BackdropSetup.TryApply(this)) Root.Background = null; // let Mica / acrylic show through

        Activated += OnActivated;
        Closed += OnClosed;
        ContentFrame.Navigated += OnFrameNavigated;
        OnWindowCreatedExtra();
    }

    /// <summary>Phase 09 (trust gate): subscribe to app view models for this window's lifetime.</summary>
    partial void OnWindowCreatedExtra();

    /// <summary>Phase 09: undo whatever <see cref="OnWindowCreatedExtra"/> subscribed.</summary>
    partial void OnWindowClosedExtra();

    /// <summary>
    /// Phase 09: runs on every <see cref="ShowPhase"/> after the overlay and pane are set, before
    /// routing. Set <paramref name="gated"/> and navigate <c>ContentFrame</c> to show a gate page instead.
    /// </summary>
    partial void OnShowPhaseGate(AccountPhase phase, ref bool gated);

    internal void ShowPhase(AccountPhase phase, string? page = null)
    {
        if (_fatal) return;
        var app = App.Current;
        Overlay.Visibility = phase is AccountPhase.Starting or AccountPhase.Failed ? Visibility.Visible : Visibility.Collapsed;
        Spinner.IsActive = phase == AccountPhase.Starting;
        OverlayText.Text = phase == AccountPhase.Failed ? app.Bridge.StatusMessage ?? "fsync could not start." : "Starting...";
        ResetHistoryButton.Visibility = phase == AccountPhase.Failed && app.Bridge.HistoryKeyMismatch ? Visibility.Visible : Visibility.Collapsed;

        _ready = phase is AccountPhase.Ready or AccountPhase.Offline;
        foreach (var item in Nav.MenuItems.OfType<NavigationViewItem>())
            item.IsEnabled = _ready || (string)item.Tag == PairingTag && phase == AccountPhase.NeedsPairing;
        if (_navLoaded && Nav.SettingsItem is NavigationViewItem settings) settings.IsEnabled = _ready;
        Nav.IsPaneVisible = _ready;

        var gated = false;
        OnShowPhaseGate(phase, ref gated);
        if (gated) return;

        switch (phase)
        {
            case AccountPhase.SignedOut: Navigate(typeof(OnboardingPage)); break;
            case AccountPhase.NeedsPairing: Navigate(typeof(PairingPage)); break;
            case AccountPhase.Ready or AccountPhase.Offline:
                Select(page ?? _pendingTag ?? NavigationRoutes.TagFor(ContentFrame.Content?.GetType()) ?? DevicesTag);
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

    private void Select(string tag)
    {
        if (tag == SettingsTag && !_navLoaded) { _pendingTag = tag; return; }
        _pendingTag = null;
        NavigationViewItem? item = tag == SettingsTag
            ? Nav.SettingsItem as NavigationViewItem
            : Nav.MenuItems.OfType<NavigationViewItem>().FirstOrDefault(i => (string)i.Tag == tag);
        // Re-selecting the selected item raises no SelectionChanged; the frame may show another page
        // (onboarding, pairing, a gate), so navigate directly then.
        if (item is not null && !ReferenceEquals(Nav.SelectedItem, item)) Nav.SelectedItem = item;
        else Navigate(NavigationRoutes.PageFor(tag));
    }

    /// <summary>The footer Settings item exists only once the template is applied.</summary>
    private void OnNavLoaded(object sender, RoutedEventArgs e)
    {
        _navLoaded = true;
        if (Nav.SettingsItem is NavigationViewItem s)
        {
            AutomationProperties.SetName(s, "Settings"); // e2e-locked Name
            AutomationProperties.SetAutomationId(s, "NavSettings");
            s.Tag = SettingsTag;
            s.IsEnabled = _ready;
        }
        if (_pendingTag is { } tag) Select(tag);
    }

    private void OnNavSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected) Navigate(typeof(SettingsPage));
        else if (args.SelectedItem is NavigationViewItem { Tag: string tag }) Navigate(NavigationRoutes.PageFor(tag));
    }

    private void Navigate(Type page)
    {
        if (ContentFrame.Content?.GetType() != page) ContentFrame.Navigate(page);
    }

    private void OnFrameNavigated(object sender, NavigationEventArgs e) => App.Current.VisiblePage = e.SourcePageType;

    private void OnActivated(object sender, WindowActivatedEventArgs args) =>
        App.Current.IsWindowActive = args.WindowActivationState != WindowActivationState.Deactivated;

    private void OnClosed(object sender, WindowEventArgs args)
    {
        Activated -= OnActivated;
        Closed -= OnClosed;
        ContentFrame.Navigated -= OnFrameNavigated;
        App.Current.IsWindowActive = false;
        App.Current.VisiblePage = null;
        OnWindowClosedExtra();
    }

    private async void OnResetHistory(object sender, RoutedEventArgs e)
    {
        if (await Dialogs.ConfirmAsync(Content.XamlRoot, "Start a new clipboard history?",
                "The old history is kept aside but can no longer be opened. This PC will need to be paired again.",
                "Start new history"))
            await App.Current.Bridge.ResetHistoryAsync();
    }
}
