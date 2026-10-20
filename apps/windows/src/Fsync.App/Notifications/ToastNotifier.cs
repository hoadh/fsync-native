using Fsync.App.Services;
using Fsync.App.ViewModels;
using Microsoft.UI.Dispatching;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;

namespace Fsync.App.Notifications;

/// <summary>
/// Unpackaged toasts for incoming files (spec section 4). <see cref="Register"/> runs in
/// <c>Program.Main</c> before XAML starts (the click handler must exist before the COM registration);
/// when it fails, toasts are a no-op and the tray tooltip count is the only signal. Arguments are the
/// one constant of <see cref="ToastActions"/>: no sender, file names or ids in args or tag.
/// </summary>
internal static class ToastNotifier
{
    private static bool s_registered;
    private static DispatcherQueue? s_ui;
    private static string? s_registerError;

    /// <summary>True when the registration succeeded; <see cref="Show"/> does nothing otherwise.</summary>
    public static bool IsSupported => s_registered;

    /// <summary>
    /// A toast was clicked while the app runs, parsed and marshalled to the UI thread. Clicks before
    /// <see cref="Attach"/> (cold start from a toast) are dropped: the app just opens its window.
    /// </summary>
    public static event Action<ToastAction>? Invoked;

    public static void Register()
    {
        try
        {
            AppNotificationManager.Default.NotificationInvoked += OnNotificationInvoked;
            AppNotificationManager.Default.Register();
            s_registered = true;
        }
        catch (Exception e)
        {
            try { AppNotificationManager.Default.NotificationInvoked -= OnNotificationInvoked; } catch (Exception) { }
            s_registerError = $"toast: register failed {e.GetType().Name} 0x{e.HResult:X8}, toasts off";
        }
    }

    /// <summary>On quit: the next click launches a new process instead of calling this one.</summary>
    public static void Unregister()
    {
        if (!s_registered) return;
        s_registered = false;
        s_ui = null;
        try
        {
            AppNotificationManager.Default.NotificationInvoked -= OnNotificationInvoked;
            AppNotificationManager.Default.Unregister();
        }
        catch (Exception e) { AppLog.Warn($"toast: unregister failed {e.GetType().Name} 0x{e.HResult:X8}"); }
    }

    /// <summary>Clicks from now on are delivered on <paramref name="ui"/>.</summary>
    public static void Attach(DispatcherQueue ui)
    {
        s_ui = ui;
        // Register runs before the log file is open; report its result now.
        AppLog.Info(s_registerError ?? $"toast: registered={s_registered}");
    }

    /// <summary>Posts the toast. Never shows or activates the window.</summary>
    public static void Show(IncomingFilesNotice notice)
    {
        if (!s_registered) return;
        try
        {
            var toast = new AppNotificationBuilder()
                .AddArgument("action", "open-files") // = ToastActions.OpenFilesArgs
                .AddText(notice.Title)
                .AddText(notice.Body)
                .BuildNotification();
            AppNotificationManager.Default.Show(toast);
        }
        catch (Exception e) { AppLog.Warn($"toast: show failed {e.GetType().Name} 0x{e.HResult:X8}"); }
    }

    /// <summary>Background thread. Anything but the exact constant argument is ignored.</summary>
    private static void OnNotificationInvoked(AppNotificationManager sender, AppNotificationActivatedEventArgs args)
    {
        if (!ToastActions.TryParse(args.Argument, out var action)) return;
        s_ui?.TryEnqueue(() => Invoked?.Invoke(action));
    }
}
