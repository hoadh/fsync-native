using Microsoft.UI.Xaml;

namespace Fsync.App.Shell;

/// <summary>
/// Custom title bar: the window content extends under the title bar and the given drag region
/// (the AppTitleBar grid) becomes the draggable area. Caption buttons (minimize, maximize, close) stay system-drawn.
/// </summary>
internal static class TitleBarSetup
{
    public static void Apply(Window window, UIElement dragRegion)
    {
        window.ExtendsContentIntoTitleBar = true;
        window.SetTitleBar(dragRegion);
    }
}
