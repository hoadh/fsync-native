using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views.Controls;

/// <summary>
/// What a page shows instead of an empty list: a Segoe Fluent glyph (spec icon map), a title, an
/// optional body and an optional action button (shown when <see cref="ActionText"/> is set).
/// </summary>
public sealed partial class EmptyState : UserControl
{
    public static readonly DependencyProperty GlyphProperty =
        DependencyProperty.Register(nameof(Glyph), typeof(string), typeof(EmptyState), new PropertyMetadata(""));
    public static readonly DependencyProperty TitleProperty =
        DependencyProperty.Register(nameof(Title), typeof(string), typeof(EmptyState), new PropertyMetadata(""));
    public static readonly DependencyProperty BodyProperty =
        DependencyProperty.Register(nameof(Body), typeof(string), typeof(EmptyState), new PropertyMetadata(null));
    public static readonly DependencyProperty ActionTextProperty =
        DependencyProperty.Register(nameof(ActionText), typeof(string), typeof(EmptyState), new PropertyMetadata(null));

    public EmptyState() => InitializeComponent();

    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }
    /// <summary>Nullable so a null from x:Bind cannot throw; shows nothing then.</summary>
    public string? Title { get => (string?)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }
    public string? Body { get => (string?)GetValue(BodyProperty); set => SetValue(BodyProperty, value); }
    public string? ActionText { get => (string?)GetValue(ActionTextProperty); set => SetValue(ActionTextProperty, value); }

    /// <summary>The action button was clicked.</summary>
    public event RoutedEventHandler? ActionClick;

    private void OnActionClick(object sender, RoutedEventArgs e) => ActionClick?.Invoke(this, e);
}
