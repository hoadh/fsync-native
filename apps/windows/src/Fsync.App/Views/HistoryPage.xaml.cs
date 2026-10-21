using System.Collections.Specialized;
using System.ComponentModel;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class HistoryPage : Page, INotifyPropertyChanged
{
    public HistoryViewModel ViewModel { get; } = new(App.Current.Bridge, App.Current.Clipboard);

    /// <summary>The first load on this page finished.</summary>
    private bool _loaded;
    private bool _showEmpty;

    /// <summary>A load finished with no items and no error (title and glyph follow the search filter).</summary>
    public bool ShowEmpty
    {
        get => _showEmpty;
        private set
        {
            if (_showEmpty == value) return;
            _showEmpty = value;
            PropertyChanged?.Invoke(this, new(nameof(ShowEmpty)));
            PropertyChanged?.Invoke(this, new(nameof(StatusCaption)));
        }
    }

    /// <summary>The view model's status as a caption; hidden while the empty state says the same.</summary>
    public string? StatusCaption => ShowEmpty ? null : ViewModel.Status;

    public event PropertyChangedEventHandler? PropertyChanged;

    public HistoryPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Clipboard history");
        Loaded += OnLoaded;
        Unloaded += (_, _) => Unsubscribe();
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            Unsubscribe(); // Loaded can repeat without a new page
            ViewModel.Items.CollectionChanged += OnItemsChanged;
            ViewModel.PropertyChanged += OnViewModelChanged;
            await ViewModel.RefreshAsync();
            _loaded = true;
            UpdateShowEmpty();
        }
        catch (Exception ex)
        {
            AppLog.Warn($"history: page load failed {ex.GetType().Name}");
        }
    }

    private void Unsubscribe()
    {
        ViewModel.Items.CollectionChanged -= OnItemsChanged;
        ViewModel.PropertyChanged -= OnViewModelChanged;
    }

    /// <summary>
    /// A reload clears the list before the next page arrives (across an await): a Reset keeps the current
    /// state, and the load's Status / Error change decides. Adds and deletes decide at once.
    /// </summary>
    private void OnItemsChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (e.Action != NotifyCollectionChangedAction.Reset) UpdateShowEmpty();
    }

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(HistoryViewModel.Status) or nameof(HistoryViewModel.Error)
            or nameof(HistoryViewModel.LoadedQuery))) return;
        UpdateShowEmpty();
        PropertyChanged?.Invoke(this, new(nameof(StatusCaption)));
    }

    private void UpdateShowEmpty() => ShowEmpty = _loaded && ViewModel.Items.Count == 0 && ViewModel.Error is null;

    private static HistoryRow? Row(object sender) => (sender as FrameworkElement)?.Tag as HistoryRow;

    private async void OnCopy(object sender, RoutedEventArgs e)
    {
        if (Row(sender) is { } r) await ViewModel.CopyAsync(r);
    }

    private async void OnPin(object sender, RoutedEventArgs e)
    {
        if (Row(sender) is { } r) await ViewModel.TogglePinAsync(r);
    }

    private async void OnDelete(object sender, RoutedEventArgs e)
    {
        if (Row(sender) is { } r) await ViewModel.DeleteAsync(r);
    }

    private async void OnClear(object sender, RoutedEventArgs e)
    {
        var keep = new CheckBox { Content = "Keep pinned items", IsChecked = true };
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "Clear the clipboard history on this PC?",
            Content = keep,
            PrimaryButtonText = "Clear",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await Dialogs.ShowAsync(dialog) == ContentDialogResult.Primary) await ViewModel.ClearAsync(keep.IsChecked == true);
    }
}
