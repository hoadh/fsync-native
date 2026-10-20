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
    /// <summary>How long after a launch from a toast its own click event may still arrive.</summary>
    private const long ColdStartClickWindowMs = 15_000;

    private static volatile bool s_registered;
    private static volatile DispatcherQueue? s_ui;
    private static string? s_registerResult;
    private static int s_registerLogged;
    /// <summary>Environment.TickCount64 until which the first click is the launching one; 0 = none.</summary>
    private static long s_dropColdStartClickUntil;

    /// <summary>True when the registration succeeded; <see cref="Show"/> does nothing otherwise.</summary>
    public static bool IsSupported => s_registered;

    /// <summary>A toast was clicked while the app runs, parsed and marshalled to the UI thread.</summary>
    public static event Action<ToastAction>? Invoked;

    /// <param name="launchedFromToast">This process was started by a toast click (app was closed).</param>
    public static void Register(bool launchedFromToast)
    {
        // Cold start from a toast: the click that launched us is also raised as NotificationInvoked
        // (the handler is attached before Register). It must only open the main window, which the
        // normal start does, so that one click is dropped instead of navigating to Files. The window
        // is bounded so a launch whose click never arrives cannot swallow a later real click.
        if (launchedFromToast) Interlocked.Exchange(ref s_dropColdStartClickUntil, Environment.TickCount64 + ColdStartClickWindowMs);
        try
        {
            AppNotificationManager.Default.NotificationInvoked += OnNotificationInvoked;
            AppNotificationManager.Default.Register();
            s_registered = true;
            s_registerResult = $"toast: registered (launched from toast: {launchedFromToast})";
        }
        catch (Exception e)
        {
            try { AppNotificationManager.Default.NotificationInvoked -= OnNotificationInvoked; } catch (Exception) { }
            s_registerResult = $"toast: register failed {e.GetType().Name} 0x{e.HResult:X8}, toasts off";
        }
    }

    /// <summary>On quit: the next click launches a new process instead of calling this one.</summary>
    public static void Unregister()
    {
        LogRegisterResult(); // Attach may never have run (engine failed to load)
        s_ui = null;
        if (!s_registered) return;
        s_registered = false;
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
        LogRegisterResult();
    }

    /// <summary>Posts the toast. Never shows or activates the window.</summary>
    public static void Show(IncomingFilesNotice notice)
    {
        if (!s_registered) return;
        try
        {
            var toast = new AppNotificationBuilder()
                .AddArgument(ToastActions.ArgumentKey, ToastActions.OpenFilesValue) // = ToastActions.OpenFilesArgs
                .AddText(notice.Title)
                .AddText(notice.Body)
                .BuildNotification();
            AppNotificationManager.Default.Show(toast);
        }
        catch (Exception e) { AppLog.Warn($"toast: show failed {e.GetType().Name} 0x{e.HResult:X8}"); }
    }

    /// <summary>Register runs before the log file is open; its result is logged once, as soon as possible.</summary>
    private static void LogRegisterResult()
    {
        if (s_registerResult is { } r && Interlocked.Exchange(ref s_registerLogged, 1) == 0) AppLog.Info(r);
    }

    /// <summary>Background thread. Anything but the exact constant argument is ignored.</summary>
    private static void OnNotificationInvoked(AppNotificationManager sender, AppNotificationActivatedEventArgs args)
    {
        var dropUntil = Interlocked.Exchange(ref s_dropColdStartClickUntil, 0);
        if (dropUntil != 0 && Environment.TickCount64 <= dropUntil) return; // the launching click
        if (!ToastActions.TryParse(args.Argument, out var action)) return;
        s_ui?.TryEnqueue(() => Invoked?.Invoke(action));
    }
}
