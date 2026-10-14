using System.Runtime.InteropServices;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

/// <summary>
/// ContentDialog.ShowAsync throws when another dialog is open (e.g. a double click on a button
/// that asks for confirmation); unhandled, that ends the app. Every dialog goes through here.
/// </summary>
public static class Dialogs
{
    private static bool s_open;

    /// <summary>The user's answer, or None when another dialog is already showing.</summary>
    public static async Task<ContentDialogResult> ShowAsync(ContentDialog dialog)
    {
        if (s_open) return ContentDialogResult.None;
        s_open = true;
        try { return await dialog.ShowAsync(); }
        catch (COMException) { return ContentDialogResult.None; }
        finally { s_open = false; }
    }
}
