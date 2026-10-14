using System.ComponentModel;
using System.Diagnostics;
using Windows.Win32;
using Windows.Win32.Foundation;

namespace Fsync.Interop.Clipboard;

/// <summary>Which process put the current clipboard content there (for the app denylist).</summary>
public static unsafe class ClipboardOwner
{
    /// <summary>Process id of the clipboard owner window, or null (no owner, e.g. a NULL-owner write).</summary>
    public static uint? ProcessId() => Pid(PInvoke.GetClipboardOwner());

    /// <summary>Process id of the foreground window (who most likely copied), or null.</summary>
    public static uint? ForegroundProcessId() => Pid(PInvoke.GetForegroundWindow());

    /// <summary>Exe name without extension ("KeePass") of the owner, or null when unknown.</summary>
    public static string? ProcessName() => NameOf(ProcessId());

    public static string? ForegroundProcessName() => NameOf(ForegroundProcessId());

    private static uint? Pid(HWND hwnd)
    {
        if (hwnd.IsNull) return null;
        uint pid = 0;
        return PInvoke.GetWindowThreadProcessId(hwnd, &pid) == 0 || pid == 0 ? null : pid;
    }

    private static string? NameOf(uint? pid)
    {
        if (pid is not { } id) return null;
        try
        {
            using var p = Process.GetProcessById((int)id);
            return p.ProcessName;
        }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException or Win32Exception)
        {
            return null; // exited meanwhile, or protected
        }
    }
}
