using System.Runtime.InteropServices;
using System.Text;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.System.Memory;

namespace ClipSpike;

/// <summary>
/// Plain Win32 clipboard writes (no OLE), used by the self-test to imitate other apps, e.g. a
/// password manager that adds ExcludeClipboardContentFromMonitorProcessing next to the text.
/// </summary>
public static unsafe class ClipboardWriter
{
    public const uint CF_UNICODETEXT = 13;

    /// <summary>Text plus extra named formats (DWORD values), in one clipboard transaction.</summary>
    public static void Write(string text, params (string Name, uint Value)[] flags)
    {
        // A window owner is required: with a NULL owner EmptyClipboard makes SetClipboardData fail.
        fixed (char* cls = "STATIC")
        fixed (char* title = "")
        {
            var hwnd = PInvoke.CreateWindowEx(0, cls, title, 0, 0, 0, 0, 0, HWND.HWND_MESSAGE, default, default, null);
            if (hwnd.IsNull) throw new InvalidOperationException($"CreateWindowEx: {Marshal.GetLastWin32Error()}");
            try
            {
                for (var i = 0; !PInvoke.OpenClipboard(hwnd); i++)
                {
                    if (i > 20) throw new InvalidOperationException("OpenClipboard failed");
                    Thread.Sleep(25);
                }
                try
                {
                    PInvoke.EmptyClipboard();
                    Set(CF_UNICODETEXT, Encoding.Unicode.GetBytes(text + "\0"));
                    foreach (var (name, value) in flags)
                        Set(SensitiveFormatFilter.Register(name), BitConverter.GetBytes(value));
                }
                finally
                {
                    PInvoke.CloseClipboard();
                }
            }
            finally
            {
                PInvoke.DestroyWindow(hwnd);
            }
        }
    }

    private static void Set(uint format, byte[] bytes)
    {
        var h = PInvoke.GlobalAlloc(GLOBAL_ALLOC_FLAGS.GMEM_MOVEABLE, (nuint)bytes.Length);
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
