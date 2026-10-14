using System.Runtime.InteropServices;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.UI.WindowsAndMessaging;

namespace ClipSpike;

/// <summary>
/// Message-only window (HWND_MESSAGE) registered with AddClipboardFormatListener.
/// Create it on a thread that pumps messages (WinUI UI thread, or <see cref="MessagePump"/>);
/// <see cref="Changed"/> fires on that thread for every WM_CLIPBOARDUPDATE.
/// </summary>
public sealed unsafe class ClipboardListener : IDisposable
{
    private static readonly string ClassName = "FsyncClipSpikeListener";
    private static WNDPROC? s_proc; // rooted: native code holds the pointer
    private static readonly Dictionary<nint, ClipboardListener> s_byHwnd = new();
    private HWND _hwnd;

    /// <summary>Argument: GetClipboardSequenceNumber at the time of the update.</summary>
    public event Action<uint>? Changed;

    public ClipboardListener()
    {
        var hInstance = PInvoke.GetModuleHandle((char*)null);
        if (s_proc is null)
        {
            s_proc = WndProc;
            fixed (char* name = ClassName)
            {
                var wc = new WNDCLASSW
                {
                    lpfnWndProc = s_proc,
                    hInstance = (HINSTANCE)hInstance.Value,
                    lpszClassName = name,
                };
                if (PInvoke.RegisterClass(wc) == 0) throw new InvalidOperationException($"RegisterClass: {Marshal.GetLastWin32Error()}");
            }
        }
        fixed (char* cls = ClassName)
        fixed (char* title = "")
        {
            _hwnd = PInvoke.CreateWindowEx(0, cls, title, 0, 0, 0, 0, 0,
                HWND.HWND_MESSAGE, default, (HINSTANCE)hInstance.Value, null);
        }
        if (_hwnd.IsNull) throw new InvalidOperationException($"CreateWindowEx: {Marshal.GetLastWin32Error()}");
        lock (s_byHwnd) s_byHwnd[_hwnd] = this;
        if (!PInvoke.AddClipboardFormatListener(_hwnd))
            throw new InvalidOperationException($"AddClipboardFormatListener: {Marshal.GetLastWin32Error()}");
    }

    private static LRESULT WndProc(HWND hwnd, uint msg, WPARAM w, LPARAM l)
    {
        if (msg == PInvoke.WM_CLIPBOARDUPDATE)
        {
            ClipboardListener? self;
            lock (s_byHwnd) s_byHwnd.TryGetValue(hwnd, out self);
            self?.Changed?.Invoke(PInvoke.GetClipboardSequenceNumber());
            return (LRESULT)0;
        }
        return PInvoke.DefWindowProc(hwnd, msg, w, l);
    }

    public void Dispose()
    {
        if (_hwnd.IsNull) return;
        PInvoke.RemoveClipboardFormatListener(_hwnd);
        lock (s_byHwnd) s_byHwnd.Remove(_hwnd);
        PInvoke.DestroyWindow(_hwnd);
        _hwnd = default;
    }
}
