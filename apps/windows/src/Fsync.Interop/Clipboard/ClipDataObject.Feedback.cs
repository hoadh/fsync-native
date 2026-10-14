using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

namespace Fsync.Interop.Clipboard;

/// <summary>Shell feedback (SetData) and IDataObjectAsyncCapability for <see cref="ClipDataObject"/>.</summary>
public sealed partial class ClipDataObject : IDataObjectAsyncCapability
{
    private readonly short _cfPerformed, _cfPasteOk;
    private readonly Dictionary<short, byte[]> _received = [];
    private bool _asyncMode = true, _inOperation;

    /// <summary>Performed DropEffect the shell reported after a paste, if any.</summary>
    public uint? PerformedDropEffect => Dword(_cfPerformed);
    public uint? PasteSucceeded => Dword(_cfPasteOk);
    public bool OperationStarted { get; private set; }
    /// <summary>HRESULT of the async extraction (paste) once it ended.</summary>
    public int? OperationResult { get; private set; }
    /// <summary>Set by EndOperation: the async extraction is over, successfully or not.</summary>
    public ManualResetEventSlim OperationEnded { get; } = new();

    private byte[]? Received(short cf)
    {
        lock (_received) return _received.GetValueOrDefault(cf);
    }

    private uint? Dword(short cf) => Received(cf) is { Length: >= 4 } b ? BitConverter.ToUInt32(b) : null;

    public void SetData(ref FORMATETC format, ref STGMEDIUM medium, bool release)
    {
        // Shell feedback (Performed DropEffect, Paste Succeeded, DropDescription, ...): keep a copy.
        if (medium.tymed == TYMED.TYMED_HGLOBAL && medium.unionmember != IntPtr.Zero)
        {
            var size = (int)Math.Min((long)GlobalSize(medium.unionmember), 64 * 1024);
            var bytes = new byte[size];
            var p = GlobalLock(medium.unionmember);
            if (p != IntPtr.Zero)
            {
                try { Marshal.Copy(p, bytes, 0, size); }
                finally { GlobalUnlock(medium.unionmember); }
                lock (_received) _received[format.cfFormat] = bytes;
            }
        }
        Log?.Invoke($"SetData {ClipboardFormats.Name((ushort)format.cfFormat)}");
        if (release) ReleaseStgMedium(ref medium);
    }

    // IDataObjectAsyncCapability: lets Explorer extract on a background thread (no UI freeze).
    public int SetAsyncMode(int fDoOpAsync) { _asyncMode = fDoOpAsync != 0; return S_OK; }
    public int GetAsyncMode(out int pfIsOpAsync) { pfIsOpAsync = _asyncMode ? 1 : 0; return S_OK; }
    public int StartOperation(IntPtr pbcReserved) { _inOperation = true; OperationStarted = true; Log?.Invoke("StartOperation"); return S_OK; }
    public int InOperation(out int pfInAsyncOp) { pfInAsyncOp = _inOperation ? 1 : 0; return S_OK; }
    public int EndOperation(int hResult, IntPtr pbcReserved, uint dwEffects)
    {
        _inOperation = false;
        OperationResult = hResult;
        Log?.Invoke($"EndOperation 0x{hResult:X8}");
        OperationEnded.Set();
        return S_OK;
    }

    [DllImport("kernel32.dll")] private static extern nuint GlobalSize(IntPtr h);
    [DllImport("kernel32.dll")] private static extern IntPtr GlobalLock(IntPtr h);
    [DllImport("kernel32.dll")] private static extern bool GlobalUnlock(IntPtr h);
    [DllImport("ole32.dll")] private static extern void ReleaseStgMedium(ref STGMEDIUM medium);
}

/// <summary>IDataObjectAsyncCapability (shobjidl.h), not in ComTypes.</summary>
[ComImport, Guid("3D8B0590-F691-11d2-8EA9-006097DF5BD4"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IDataObjectAsyncCapability
{
    [PreserveSig] int SetAsyncMode(int fDoOpAsync);
    [PreserveSig] int GetAsyncMode(out int pfIsOpAsync);
    [PreserveSig] int StartOperation(IntPtr pbcReserved);
    [PreserveSig] int InOperation(out int pfInAsyncOp);
    [PreserveSig] int EndOperation(int hResult, IntPtr pbcReserved, uint dwEffects);
}

internal sealed class FormatEnumerator(FORMATETC[] items) : IEnumFORMATETC
{
    private int _i;
    public int Next(int celt, FORMATETC[] rgelt, int[]? pceltFetched)
    {
        var n = 0;
        for (; n < celt && _i < items.Length; n++, _i++) rgelt[n] = items[_i];
        if (pceltFetched is { Length: > 0 }) pceltFetched[0] = n;
        return n == celt ? 0 : 1;
    }
    public int Skip(int celt) { _i += celt; return _i <= items.Length ? 0 : 1; }
    public int Reset() { _i = 0; return 0; }
    public void Clone(out IEnumFORMATETC newEnum) => newEnum = new FormatEnumerator(items) { _i = _i };
}
