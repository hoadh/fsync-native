using Microsoft.UI.Dispatching;

namespace ClipSpike.Tray;

/// <summary>
/// Go/No-Go checks that need a real WinUI 3 process: tray icon created, WM_CLIPBOARDUPDATE
/// delivered on the UI thread while XAML runs the message loop, OLE clipboard ownership from
/// the UI thread, exit policy (clear our lazy entry).
/// </summary>
internal static class SmokeTest
{
    public static async Task RunAsync(App app, DispatcherQueue ui, string logPath)
    {
        var lines = new List<string>();
        var ok = true;
        void Check(string name, bool pass, string detail)
        {
            ok &= pass;
            lines.Add($"{(pass ? "PASS" : "FAIL")} {name}: {detail}");
        }

        await Task.Delay(500);
        Check("tray icon created (H.NotifyIcon in WinUI 3)", app.IconCreated, $"IsCreated={app.IconCreated}");

        // Other-thread writes, as another app would do.
        await Task.Run(() =>
        {
            for (var i = 0; i < 20; i++) { ClipboardWriter.Write($"smoke {i}"); Thread.Sleep(30); }
            ClipboardWriter.Write("pw", (SensitiveFormatFilter.ExcludeFromMonitor, 0));
        });
        await Task.Delay(1000);
        var events = app.Events.ToList();
        Check("listener on UI thread: 21 writes seen", events.Count >= 21 && events.All(e => e.OnUiThread),
            $"{events.Count} updates, all on UI thread={events.All(e => e.OnUiThread)}");
        Check("filter in WinUI process", events.LastOrDefault().Reason == SkipReason.ExcludeFromMonitor, $"last={events.LastOrDefault().Reason}");

        // Own lazy entry from the UI thread, then the exit policy.
        VirtualFileDataObject? obj = null;
        var done = new TaskCompletionSource();
        ui.TryEnqueue(() =>
        {
            obj = new VirtualFileDataObject([new VirtualFile("tray.bin", 4096, DateTime.UtcNow, (o, n) => new byte[n])]);
            OleClipboard.Set(obj);
            done.SetResult();
        });
        await done.Task;
        await Task.Delay(500);
        Check("own entry seen as echo", app.Events.LastOrDefault().Reason == SkipReason.OwnEcho, $"last={app.Events.LastOrDefault().Reason}");

        var cleared = new TaskCompletionSource<bool>();
        ui.TryEnqueue(() =>
        {
            OleClipboard.ClearIfCurrent(obj!);
            cleared.SetResult(!OleClipboard.IsCurrent(obj!));
        });
        Check("exit policy: clear our lazy entry", await cleared.Task, "OleSetClipboard(NULL) while current");

        lines.Add(ok ? "RESULT PASS" : "RESULT FAIL");
        await File.WriteAllLinesAsync(logPath, lines);
        ui.TryEnqueue(() =>
        {
            app.Shutdown();
            Environment.Exit(ok ? 0 : 1);
        });
    }
}
