using Fsync.App.Bridge;
using Fsync.App.Notifications;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using WinRT;

namespace Fsync.App;

/// <summary>
/// Single instance per data directory: a second launch hands its activation to the running
/// app (which shows its window) and exits before XAML starts.
/// </summary>
public static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        ComWrappersSupport.InitializeComWrappers();
        var settings = AppSettings.FromEnvironment();
        var instance = AppInstance.FindOrRegisterForKey(settings.InstanceKey);
        if (!instance.IsCurrent)
        {
            // Redirect synchronously: Main must not return before the hand-off is done.
            var activation = AppInstance.GetCurrent().GetActivatedEventArgs();
            var done = new ManualResetEventSlim();
            Task.Run(async () =>
            {
                await instance.RedirectActivationToAsync(activation);
                done.Set();
            });
            done.Wait(TimeSpan.FromSeconds(5));
            return 0;
        }

        // Toasts: unpackaged registration must precede the app; a failure only turns toasts off.
        // A launch from a toast (app closed) arrives as a plain start: it just opens the window.
        ToastNotifier.Register();
        var background = args.Contains("--background");
        try
        {
            Application.Start(_ =>
            {
                var context = new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread());
                SynchronizationContext.SetSynchronizationContext(context);
                var app = new App(settings, showWindowAtStart: !background, args);
                instance.Activated += (_, e) => app.ShowFromAnotherInstance(LaunchArgs(e));
            });
        }
        finally { ToastNotifier.Unregister(); }
        return 0;
    }

    /// <summary>Command line of a redirected launch (Explorer "Send to" passes the paths there).</summary>
    private static string[] LaunchArgs(AppActivationArguments e) =>
        e.Kind == ExtendedActivationKind.Launch && e.Data is Windows.ApplicationModel.Activation.ILaunchActivatedEventArgs l
            ? Files.SendToArgs.Split(l.Arguments)
            : [];
}
