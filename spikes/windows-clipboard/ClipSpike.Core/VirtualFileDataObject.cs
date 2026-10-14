using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

namespace ClipSpike;

/// <summary>
/// Delayed-render clipboard object for remote files: FileGroupDescriptorW now, FileContents
/// (one IStream per lindex) only when a target pastes. Also serves Preferred DropEffect=COPY and
/// the FsyncOrigin marker, and accepts the shell's SetData feedback (Performed DropEffect, ...).
/// Must be put on the clipboard from an STA thread that keeps pumping messages.
/// </summary>
[ComVisible(true)]
public sealed class VirtualFileDataObject : IDataObject, IDataObjectAsyncCapability
{
    private const int S_OK = 0, S_FALSE = 1;
    private const int DV_E_FORMATETC = unchecked((int)0x80040064);
    private const int DV_E_LINDEX = unchecked((int)0x80040068);
    private const int DV_E_TYMED = unchecked((int)0x80040069);
    private const int E_NOTIMPL = unchecked((int)0x80004001);
    private const int OLE_E_ADVISENOTSUPPORTED = unchecked((int)0x80040003);

    private readonly IReadOnlyList<VirtualFile> _files;
    private readonly short _cfDescriptor, _cfContents, _cfDropEffect, _cfOrigin, _cfPerformed, _cfPasteOk;
    private readonly Dictionary<short, byte[]> _received = new();
    private readonly List<RemoteFileStream> _streams = [];
    private bool _asyncMode = true, _inOperation;

    public VirtualFileDataObject(IReadOnlyList<VirtualFile> files)
    {
        _files = files;
        _cfDescriptor = Cf(ShellFormats.FileDescriptorW);
        _cfContents = Cf(ShellFormats.FileContents);
        _cfDropEffect = Cf(ShellFormats.PreferredDropEffect);
        _cfOrigin = Cf(SensitiveFormatFilter.FsyncOrigin);
        _cfPerformed = Cf(ShellFormats.PerformedDropEffect);
        _cfPasteOk = Cf(ShellFormats.PasteSucceeded);
    }

    private static short Cf(string name) => unchecked((short)SensitiveFormatFilter.Register(name));

    /// <summary>Spike log: every format request and SetData, in order (names only, no content).</summary>
    public event Action<string>? Log;
    public IReadOnlyList<RemoteFileStream> Streams { get { lock (_streams) return [.. _streams]; } }
    public uint? PerformedDropEffect => Received(_cfPerformed);
    public uint? PasteSucceeded => Received(_cfPasteOk);
    public bool OperationStarted { get; private set; }
    public int? OperationResult { get; private set; }
    /// <summary>Set by EndOperation: the async extraction (paste) is over, successfully or not.</summary>
    public ManualResetEventSlim OperationEnded { get; } = new();

    /// <summary>Fails all streams handed out so far (cancel, or app exit with a lazy entry).</summary>
    public void Abort() { foreach (var s in Streams) s.Aborted = true; }

    private uint? Received(short cf)
    {
        lock (_received) return _received.TryGetValue(cf, out var b) && b.Length >= 4 ? BitConverter.ToUInt32(b) : null;
    }

    private IEnumerable<(short cf, TYMED tymed, int lindex)> Offered()
    {
        yield return (_cfDescriptor, TYMED.TYMED_HGLOBAL, -1);
        for (var i = 0; i < _files.Count; i++) yield return (_cfContents, TYMED.TYMED_ISTREAM, i);
        yield return (_cfDropEffect, TYMED.TYMED_HGLOBAL, -1);
        yield return (_cfOrigin, TYMED.TYMED_HGLOBAL, -1);
    }

    public void GetData(ref FORMATETC format, out STGMEDIUM medium)
    {
        var hr = QueryGetData(ref format);
        Log?.Invoke($"GetData {SensitiveFormatFilter.FormatName((ushort)format.cfFormat)} lindex={format.lindex} -> 0x{hr:X8}");
        if (hr != S_OK) Marshal.ThrowExceptionForHR(hr);

        medium = new STGMEDIUM { pUnkForRelease = null };
        if (format.cfFormat == _cfContents)
        {
            var f = _files[format.lindex];
            var s = new RemoteFileStream(f.Name, f.Length, f.Reader);
            lock (_streams) _streams.Add(s);
            medium.tymed = TYMED.TYMED_ISTREAM;
            medium.unionmember = Marshal.GetComInterfaceForObject(s, typeof(IStream));
            return;
        }
        medium.tymed = TYMED.TYMED_HGLOBAL;
        medium.unionmember = format.cfFormat == _cfDescriptor ? ShellFormats.FileGroupDescriptor(_files)
            : format.cfFormat == _cfDropEffect ? ShellFormats.Dword(ShellFormats.DROPEFFECT_COPY)
            : ShellFormats.Dword(1); // FsyncOrigin marker
    }

    public int QueryGetData(ref FORMATETC format)
    {
        if (format.dwAspect != DVASPECT.DVASPECT_CONTENT) return DV_E_FORMATETC;
        var cf = format.cfFormat;
        if (cf == _cfContents)
        {
            if (format.lindex < 0 || format.lindex >= _files.Count) return DV_E_LINDEX;
            return (format.tymed & TYMED.TYMED_ISTREAM) != 0 ? S_OK : DV_E_TYMED;
        }
        if (cf == _cfDescriptor || cf == _cfDropEffect || cf == _cfOrigin)
            return (format.tymed & TYMED.TYMED_HGLOBAL) != 0 ? S_OK : DV_E_TYMED;
        lock (_received) if (_received.ContainsKey(cf)) return S_OK;
        return DV_E_FORMATETC;
    }

    public void SetData(ref FORMATETC format, ref STGMEDIUM medium, bool release)
    {
        // Shell feedback (Performed DropEffect, Paste Succeeded, ...): keep a copy of HGLOBALs.
        if (medium.tymed == TYMED.TYMED_HGLOBAL && medium.unionmember != IntPtr.Zero)
        {
            var size = (int)GlobalSize(medium.unionmember);
            var bytes = new byte[size];
            var p = GlobalLock(medium.unionmember);
            try { Marshal.Copy(p, bytes, 0, size); } finally { GlobalUnlock(medium.unionmember); }
            lock (_received) _received[format.cfFormat] = bytes;
        }
        Log?.Invoke($"SetData {SensitiveFormatFilter.FormatName((ushort)format.cfFormat)}");
        if (release) ReleaseStgMedium(ref medium);
    }

    public IEnumFORMATETC EnumFormatEtc(DATADIR direction)
    {
        if (direction != DATADIR.DATADIR_GET) Marshal.ThrowExceptionForHR(E_NOTIMPL);
        // lindex -1 for FileContents in the enumeration, as the shell expects.
        var list = Offered().Where(o => o.lindex <= 0)
            .Select(o => new FORMATETC { cfFormat = o.cf, dwAspect = DVASPECT.DVASPECT_CONTENT, lindex = -1, tymed = o.tymed })
            .ToArray();
        return new FormatEnumerator(list);
    }

    public int GetCanonicalFormatEtc(ref FORMATETC formatIn, out FORMATETC formatOut)
    {
        formatOut = formatIn;
        formatOut.ptd = IntPtr.Zero;
        return unchecked((int)0x00040130); // DATA_S_SAMEFORMATETC
    }

    public void GetDataHere(ref FORMATETC format, ref STGMEDIUM medium) => Marshal.ThrowExceptionForHR(E_NOTIMPL);
    public int DAdvise(ref FORMATETC pFormatetc, ADVF advf, IAdviseSink adviseSink, out int connection)
    {
        connection = 0;
        return OLE_E_ADVISENOTSUPPORTED;
    }
    public void DUnadvise(int connection) => Marshal.ThrowExceptionForHR(OLE_E_ADVISENOTSUPPORTED);
    public int EnumDAdvise(out IEnumSTATDATA? enumAdvise)
    {
        enumAdvise = null;
        return OLE_E_ADVISENOTSUPPORTED;
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
        Log?.Invoke($"EndOperation hr=0x{hResult:X8} effects={dwEffects}");
        OperationEnded.Set();
        return S_OK;
    }

    [DllImport("kernel32.dll")] private static extern nuint GlobalSize(IntPtr h);
    [DllImport("kernel32.dll")] private static extern IntPtr GlobalLock(IntPtr h);
    [DllImport("kernel32.dll")] private static extern bool GlobalUnlock(IntPtr h);
    [DllImport("ole32.dll")] private static extern void ReleaseStgMedium(ref STGMEDIUM medium);

    private sealed class FormatEnumerator(FORMATETC[] items) : IEnumFORMATETC
    {
        private int _i;
        public int Next(int celt, FORMATETC[] rgelt, int[]? pceltFetched)
        {
            var n = 0;
            for (; n < celt && _i < items.Length; n++, _i++) rgelt[n] = items[_i];
            if (pceltFetched is { Length: > 0 }) pceltFetched[0] = n;
            return n == celt ? S_OK : S_FALSE;
        }
        public int Skip(int celt) { _i += celt; return _i <= items.Length ? S_OK : S_FALSE; }
        public int Reset() { _i = 0; return S_OK; }
        public void Clone(out IEnumFORMATETC newEnum) => newEnum = new FormatEnumerator(items) { _i = _i };
    }
}

/// <summary>IDataObjectAsyncCapability (shldisp.h / shobjidl.h), not in ComTypes.</summary>
[ComImport, Guid("3D8B0590-F691-11d2-8EA9-006097DF5BD4"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IDataObjectAsyncCapability
{
    [PreserveSig] int SetAsyncMode(int fDoOpAsync);
    [PreserveSig] int GetAsyncMode(out int pfIsOpAsync);
    [PreserveSig] int StartOperation(IntPtr pbcReserved);
    [PreserveSig] int InOperation(out int pfInAsyncOp);
    [PreserveSig] int EndOperation(int hResult, IntPtr pbcReserved, uint dwEffects);
}
