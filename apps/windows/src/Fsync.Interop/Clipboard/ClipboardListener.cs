using System.Runtime.InteropServices;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.UI.Input.KeyboardAndMouse;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// Message-only window (HWND_MESSAGE) registered with AddClipboardFormatListener; also the
/// target of global hotkeys (RegisterHotKey). Create and dispose it on a thread that pumps
/// messages (<see cref="StaThread"/>); events fire on that thread.
/// </summary>
public sealed unsafe class ClipboardListener : IDisposable
{
    private const string ClassName = "FsyncClipboardListener";
    private static WNDPROC? s_proc; // rooted: native code holds the pointer
    private static readonly Dictionary<nint, ClipboardListener> s_byHwnd = [];
    private readonly HashSet<int> _hotkeys = [];
    private HWND _hwnd;

    /// <summary>Argument: GetClipboardSequenceNumber at the time of the update.</summary>
    public event Action<uint>? Changed;
    /// <summary>Argument: the id given to <see cref="RegisterHotKey"/>.</summary>
    public event Action<int>? HotKey;

    public ClipboardListener()
    {
        var hInstance = PInvoke.GetModuleHandle((char*)null);
        lock (s_byHwnd)
        {
            if (s_proc is null)
            {
                s_proc = WndProc;
                fixed (char* name = ClassName)
                {
                    var wc = new WNDCLASSW { lpfnWndProc = s_proc, hInstance = (HINSTANCE)hInstance.Value, lpszClassName = name };
                    if (PInvoke.RegisterClass(wc) == 0) throw new InvalidOperationException($"RegisterClass: {Marshal.GetLastWin32Error()}");
                }
            }
        }
        fixed (char* cls = ClassName)
        fixed (char* title = "")
        {
            _hwnd = PInvoke.CreateWindowEx(0, cls, title, 0, 0, 0, 0, 0, HWND.HWND_MESSAGE, default, (HINSTANCE)hInstance.Value, null);
        }
        if (_hwnd.IsNull) throw new InvalidOperationException($"CreateWindowEx: {Marshal.GetLastWin32Error()}");
        lock (s_byHwnd) s_byHwnd[_hwnd] = this;
        if (!PInvoke.AddClipboardFormatListener(_hwnd))
            throw new InvalidOperationException($"AddClipboardFormatListener: {Marshal.GetLastWin32Error()}");
    }

    /// <summary>Registers a system-wide hotkey; false if another app already owns the combination.</summary>
    public bool RegisterHotKey(int id, HotKeyModifiers modifiers, uint virtualKey)
    {
        var mods = (HOT_KEY_MODIFIERS)((uint)modifiers | 0x4000); // MOD_NOREPEAT
        if (!PInvoke.RegisterHotKey(_hwnd, id, mods, virtualKey)) return false;
        _hotkeys.Add(id);
        return true;
    }

    public void UnregisterHotKey(int id)
    {
        if (_hotkeys.Remove(id)) PInvoke.UnregisterHotKey(_hwnd, id);
    }

    private static LRESULT WndProc(HWND hwnd, uint msg, WPARAM w, LPARAM l)
    {
        if (msg != PInvoke.WM_CLIPBOARDUPDATE && msg != PInvoke.WM_HOTKEY) return PInvoke.DefWindowProc(hwnd, msg, w, l);
        ClipboardListener? self;
        lock (s_byHwnd) s_byHwnd.TryGetValue(hwnd, out self);
        try
        {
            if (msg == PInvoke.WM_CLIPBOARDUPDATE) self?.Changed?.Invoke(PInvoke.GetClipboardSequenceNumber());
            else self?.HotKey?.Invoke((int)w.Value);
        }
        catch (Exception e)
        {
            // Never let an exception cross the native window procedure.
            System.Diagnostics.Trace.TraceError($"clipboard listener handler: {e.GetType().Name}: {e.Message}");
        }
        return (LRESULT)0;
    }

    public void Dispose()
    {
        if (_hwnd.IsNull) return;
        foreach (var id in _hotkeys) PInvoke.UnregisterHotKey(_hwnd, id);
        _hotkeys.Clear();
        PInvoke.RemoveClipboardFormatListener(_hwnd);
        lock (s_byHwnd) s_byHwnd.Remove(_hwnd);
        PInvoke.DestroyWindow(_hwnd);
        _hwnd = default;
    }
}

/// <summary>RegisterHotKey modifiers (MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN).</summary>
[Flags]
public enum HotKeyModifiers : uint { None = 0, Alt = 1, Control = 2, Shift = 4, Win = 8 }
