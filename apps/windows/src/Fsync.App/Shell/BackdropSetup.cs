using Fsync.App.Services;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace Fsync.App.Shell;

/// <summary>Mica where supported (Windows 11), else desktop acrylic, else none (solid page background).</summary>
internal static class BackdropSetup
{
    /// <returns>True when a backdrop was set (the root must then stay transparent).</returns>
    public static bool TryApply(Window window)
    {
        try
        {
            if (MicaController.IsSupported()) window.SystemBackdrop = new MicaBackdrop();
            else if (DesktopAcrylicController.IsSupported()) window.SystemBackdrop = new DesktopAcrylicBackdrop();
            else return false;
            return true;
        }
        catch (Exception e)
        {
            // Composition can fail on remote sessions or odd drivers; a solid window is fine.
            AppLog.Warn($"backdrop: {e.GetType().Name}");
            return false;
        }
    }
}
