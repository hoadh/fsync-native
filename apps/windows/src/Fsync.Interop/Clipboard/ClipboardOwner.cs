using System.ComponentModel;
using System.Diagnostics;
using Windows.Win32;

namespace Fsync.Interop.Clipboard;

/// <summary>Which process put the current clipboard content there (for the app denylist).</summary>
public static unsafe class ClipboardOwner
{
    /// <summary>Process id of the clipboard owner window, or null (no owner, e.g. a NULL-owner write).</summary>
    public static uint? ProcessId()
    {
        var hwnd = PInvoke.GetClipboardOwner();
        if (hwnd.IsNull) return null;
        uint pid = 0;
        return PInvoke.GetWindowThreadProcessId(hwnd, &pid) == 0 || pid == 0 ? null : pid;
    }

    /// <summary>Exe name without extension ("KeePass"), or null when unknown or not accessible.</summary>
    public static string? ProcessName()
    {
        if (ProcessId() is not { } pid) return null;
        try
        {
            using var p = Process.GetProcessById((int)pid);
            return p.ProcessName;
        }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException or Win32Exception)
        {
            return null; // exited meanwhile, or protected
        }
    }
}
