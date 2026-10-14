using System.Diagnostics;
using Fsync.Interop.Clipboard;

namespace Fsync.Interop.Tests;

/// <summary>Tests that use the real system clipboard run one at a time.</summary>
[CollectionDefinition("clipboard", DisableParallelization = true)]
public sealed class ClipboardCollection;

internal static class ClipboardTestSupport
{
    public static void WriteText(string text, params (string Name, uint Value)[] flags) =>
        Win32Clipboard.Write([
            (ClipboardFormats.CF_UNICODETEXT, System.Text.Encoding.Unicode.GetBytes(text + "\0")),
            .. flags.Select(f => (ClipboardFormats.Register(f.Name), BitConverter.GetBytes(f.Value))),
        ]);

    public static ClipboardSnapshot Read(params string[] denylist) => ClipboardReader.Read(long.MaxValue, denylist);

    /// <summary>Waits for <paramref name="cond"/> (polling), false on timeout.</summary>
    public static bool WaitFor(Func<bool> cond, int ms = 5000)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < ms)
        {
            if (cond()) return true;
            Thread.Sleep(20);
        }
        return cond();
    }

    public static string TempDir()
    {
        var d = Path.Combine(Path.GetTempPath(), "fsync-interop-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(d);
        return d;
    }

    public static void DeleteDir(string d)
    {
        try { Directory.Delete(d, recursive: true); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}

/// <summary>Visible top-level windows of a process (shell dialogs of the paste helper).</summary>
internal static class ProcessWindows
{
    private delegate bool EnumProc(nint hwnd, nint lParam);
    [System.Runtime.InteropServices.DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc cb, nint lParam);
    [System.Runtime.InteropServices.DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(nint hwnd, out uint pid);
    [System.Runtime.InteropServices.DllImport("user32.dll")] private static extern bool IsWindowVisible(nint hwnd);

    /// <summary>Visible top-level windows of process <paramref name="pid"/>.</summary>
    public static int VisibleCount(uint pid)
    {
        var n = 0;
        EnumWindows((h, _) =>
        {
            GetWindowThreadProcessId(h, out var p);
            if (p == pid && IsWindowVisible(h)) n++;
            return true;
        }, 0);
        return n;
    }
}
