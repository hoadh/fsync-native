using Fsync.App.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class HistoryPage : Page
{
    public HistoryViewModel ViewModel { get; } = new(App.Current.Bridge, App.Current.Clipboard);

    public HistoryPage()
    {
        InitializeComponent();
        Loaded += async (_, _) => await ViewModel.RefreshAsync();
    }

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
