using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
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

    /// <summary>
    /// A confirmation for a destructive action: Enter / Esc answer Cancel (spec: destructive dialog
    /// rule), only an explicit click on <paramref name="primaryText"/> confirms. False when declined,
    /// closed, or another dialog is already showing.
    /// </summary>
    /// <param name="created">Gets the dialog before it shows, so the caller can <c>Hide()</c> it (e.g. its subject changed).</param>
    public static async Task<bool> ConfirmAsync(XamlRoot root, string title, string body, string primaryText, Action<ContentDialog>? created = null)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = title,
            Content = body,
            PrimaryButtonText = primaryText,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        created?.Invoke(dialog);
        return await ShowAsync(dialog) == ContentDialogResult.Primary;
    }
}
