using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// OLE clipboard calls, declared by hand with ComTypes.IDataObject: CsWin32's generated
/// IDataObject is a different type and our object implements the ComTypes one.
/// All calls must come from the STA thread that owns the clipboard entry.
/// </summary>
public static class OleClipboard
{
    private const int CLIPBRD_E_CANT_OPEN = unchecked((int)0x800401D0);

    /// <summary>Puts <paramref name="obj"/> on the clipboard (retries while another process holds it).</summary>
    public static void Set(IDataObject obj) => Retry(() => OleSetClipboard(obj));

    /// <summary>
    /// Runs a call that opens the clipboard, retrying while another process holds it open
    /// (rdpclip, clipboard managers and history open it right after every change).
    /// </summary>
    private static void Retry(Func<int> call)
    {
        for (var i = 0; ; i++)
        {
            var hr = call();
            if (hr == 0) return;
            if (hr != CLIPBRD_E_CANT_OPEN || i >= 10) Marshal.ThrowExceptionForHR(hr);
            Thread.Sleep(25);
        }
    }

    /// <summary>True while our object is still the clipboard owner.</summary>
    public static bool IsCurrent(IDataObject obj) => OleIsCurrentClipboard(obj) == 0;

    /// <summary>
    /// Renders every format into the clipboard and releases our object. Never for a virtual-file
    /// entry: FileContents cannot be flushed and the entry would be broken (ADR 010).
    /// </summary>
    public static void Flush() => Retry(OleFlushClipboard);

    /// <summary>Empties the clipboard if we own it (exit policy for lazy entries).</summary>
    public static void ClearIfCurrent(IDataObject obj) =>
        // Ownership is re-checked on every attempt: the user may copy something else while we
        // wait, and that entry must survive.
        Retry(() => IsCurrent(obj) ? OleSetClipboard(null) : 0);

    /// <summary>Reads the clipboard as an IDataObject.</summary>
    public static IDataObject Get()
    {
        Marshal.ThrowExceptionForHR(OleGetClipboard(out var obj));
        return obj;
    }

    public static void InitializeSta()
    {
        if (Thread.CurrentThread.GetApartmentState() != ApartmentState.STA)
            throw new InvalidOperationException("OLE clipboard needs an STA thread");
        var hr = OleInitialize(IntPtr.Zero);
        if (hr < 0) Marshal.ThrowExceptionForHR(hr);
    }

    public static void Uninitialize() => OleUninitialize();

    [DllImport("ole32.dll")] private static extern int OleInitialize(IntPtr reserved);
    [DllImport("ole32.dll")] private static extern void OleUninitialize();
    [DllImport("ole32.dll")] private static extern int OleSetClipboard(IDataObject? obj);
    [DllImport("ole32.dll")] private static extern int OleGetClipboard(out IDataObject obj);
    [DllImport("ole32.dll")] private static extern int OleFlushClipboard();
    [DllImport("ole32.dll")] private static extern int OleIsCurrentClipboard(IDataObject obj);
}
