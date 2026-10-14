using System.Runtime.InteropServices;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.System.Memory;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// Plain Win32 clipboard access (OpenClipboard / GetClipboardData). Another process may hold
/// the clipboard briefly, so opening retries (5 x 50 ms by default).
/// </summary>
public static unsafe class Win32Clipboard
{
    /// <summary>Runs <paramref name="body"/> with the clipboard open; false if it stayed locked.</summary>
    public static bool TryWithOpen(Action body, nint owner = 0, int attempts = 5)
    {
        for (var i = 0; !PInvoke.OpenClipboard((HWND)owner); i++)
        {
            if (i + 1 >= attempts) return false;
            Thread.Sleep(50);
        }
        try { body(); }
        finally { PInvoke.CloseClipboard(); }
        return true;
    }

    /// <summary>Formats currently on the clipboard (call with the clipboard open).</summary>
    public static List<uint> Formats()
    {
        var list = new List<uint>();
        for (var f = PInvoke.EnumClipboardFormats(0); f != 0; f = PInvoke.EnumClipboardFormats(f)) list.Add(f);
        return list;
    }

    /// <summary>HGLOBAL size of a format, or -1 (absent / GDI handle). Clipboard must be open.</summary>
    public static long Size(uint format)
    {
        if (ClipboardFormats.IsGdiHandle(format)) return -1;
        var h = PInvoke.GetClipboardData(format);
        return h.IsNull ? -1 : (long)(nuint)PInvoke.GlobalSize((HGLOBAL)h.Value);
    }

    /// <summary>Copy of a format's HGLOBAL bytes, or null. Clipboard must be open.</summary>
    public static byte[]? Read(uint format)
    {
        if (ClipboardFormats.IsGdiHandle(format)) return null;
        var h = PInvoke.GetClipboardData(format);
        if (h.IsNull) return null;
        var g = (HGLOBAL)h.Value;
        var size = (long)(nuint)PInvoke.GlobalSize(g);
        if (size <= 0 || size > int.MaxValue) return null;
        var p = PInvoke.GlobalLock(g); // handles from other processes are moveable: always lock
        if (p is null) return null;
        try
        {
            var bytes = new byte[size];
            Marshal.Copy((IntPtr)p, bytes, 0, (int)size);
            return bytes;
        }
        finally { PInvoke.GlobalUnlock(g); }
    }

    /// <summary>A DWORD flag format (CanIncludeInClipboardHistory, ...), or null.</summary>
    public static uint? ReadDword(uint format)
    {
        var b = Read(format);
        return b is { Length: >= 4 } ? BitConverter.ToUInt32(b) : null;
    }

    /// <summary>
    /// Replaces the clipboard with <paramref name="formats"/> in one transaction, as a plain
    /// Win32 writer (other apps, tests). A message-only window owns the entry.
    /// </summary>
    public static void Write(IEnumerable<(uint Format, byte[] Bytes)> formats)
    {
        fixed (char* cls = "STATIC")
        fixed (char* title = "")
        {
            // With a NULL owner EmptyClipboard makes SetClipboardData fail.
            var hwnd = PInvoke.CreateWindowEx(0, cls, title, 0, 0, 0, 0, 0, HWND.HWND_MESSAGE, default, default, null);
            if (hwnd.IsNull) throw new InvalidOperationException($"CreateWindowEx: {Marshal.GetLastWin32Error()}");
            try
            {
                var ok = TryWithOpen(() =>
                {
                    PInvoke.EmptyClipboard();
                    foreach (var (f, b) in formats) Set(f, b);
                }, hwnd, attempts: 20);
                if (!ok) throw new InvalidOperationException("the clipboard is locked by another application");
            }
            finally { PInvoke.DestroyWindow(hwnd); }
        }
    }

    private static void Set(uint format, byte[] bytes)
    {
        var h = PInvoke.GlobalAlloc(GLOBAL_ALLOC_FLAGS.GMEM_MOVEABLE, (nuint)Math.Max(bytes.Length, 1));
        if (h.IsNull) throw new OutOfMemoryException("GlobalAlloc");
        var p = PInvoke.GlobalLock(h);
        Marshal.Copy(bytes, 0, (IntPtr)p, bytes.Length);
        PInvoke.GlobalUnlock(h);
        // The clipboard owns h after a successful SetClipboardData.
        if (PInvoke.SetClipboardData(format, (HANDLE)h.Value).IsNull)
        {
            PInvoke.GlobalFree(h);
            throw new InvalidOperationException($"SetClipboardData({format}): {Marshal.GetLastWin32Error()}");
        }
    }
}
