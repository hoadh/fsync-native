using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using Fsync.Interop.Shell;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// The clipboard entry fsync writes for an incoming clip: HGLOBAL formats (eager or delayed
/// render, <see cref="ClipEntry"/>), optional virtual files (FileGroupDescriptorW + FileContents,
/// one IStream per lindex, ADR 010), Preferred DropEffect, and the FsyncOrigin marker.
/// Put it on the clipboard from an STA thread that keeps pumping (<see cref="StaThread"/>).
/// </summary>
[ComVisible(true)]
public sealed partial class ClipDataObject : IDataObject
{
    private const int S_OK = 0, S_FALSE = 1;
    private const int DV_E_FORMATETC = unchecked((int)0x80040064);
    private const int DV_E_LINDEX = unchecked((int)0x80040068);
    private const int DV_E_TYMED = unchecked((int)0x80040069);
    private const int E_FAIL = unchecked((int)0x80004005);
    private const int E_NOTIMPL = unchecked((int)0x80004001);
    private const int OLE_E_ADVISENOTSUPPORTED = unchecked((int)0x80040003);

    private readonly IReadOnlyList<ClipEntry> _entries;
    private readonly IReadOnlyList<VirtualFile> _files;
    private readonly short _cfDescriptor, _cfContents, _cfDropEffect, _cfOrigin;
    private readonly List<RemoteFileStream> _streams = [];

    public ClipDataObject(IReadOnlyList<ClipEntry> entries, IReadOnlyList<VirtualFile>? files = null)
    {
        _entries = entries;
        _files = files ?? [];
        _cfDescriptor = Cf(ShellFormats.FileDescriptorW);
        _cfContents = Cf(ShellFormats.FileContents);
        _cfDropEffect = Cf(ShellFormats.PreferredDropEffect);
        _cfOrigin = Cf(ClipboardFormats.FsyncOrigin);
        _cfPerformed = Cf(ShellFormats.PerformedDropEffect);
        _cfPasteOk = Cf(ShellFormats.PasteSucceeded);
    }

    private static short Cf(string name) => unchecked((short)ClipboardFormats.Register(name));

    public bool HasVirtualFiles => _files.Count > 0;
    public IReadOnlyList<ClipEntry> Entries => _entries;
    /// <summary>Bytes of lazy formats nobody asked for yet (exit policy).</summary>
    public long PendingLazyBytes => _entries.Where(e => !e.IsRendered).Sum(e => e.Size);
    /// <summary>A delayed render failed (format id, exception); the target sees an error.</summary>
    public event Action<uint, Exception>? RenderFailed;
    /// <summary>A target opened a file stream (lindex, stream): progress / cancel hooks.</summary>
    public event Action<int, RemoteFileStream>? StreamOpened;
    /// <summary>Diagnostics: format requests and shell feedback by name (never content).</summary>
    public event Action<string>? Log;

    /// <summary>Fails all file streams handed out so far (cancel, or app exit with a lazy entry).</summary>
    public void Abort()
    {
        lock (_streams) foreach (var s in _streams) s.Aborted = true;
    }

    private ClipEntry? Entry(short cf) => _entries.FirstOrDefault(e => unchecked((short)e.Format) == cf);

    public void GetData(ref FORMATETC format, out STGMEDIUM medium)
    {
        var hr = QueryGetData(ref format);
        Log?.Invoke($"GetData {ClipboardFormats.Name((ushort)format.cfFormat)} lindex={format.lindex} tymed={format.tymed} -> 0x{hr:X8}");
        if (hr != S_OK) Marshal.ThrowExceptionForHR(hr);
        medium = new STGMEDIUM { pUnkForRelease = null };
        if (format.cfFormat == _cfContents)
        {
            var f = _files[format.lindex];
            var s = new RemoteFileStream(Path.GetFileName(f.RelativePath), f.Length, f.Reader!);
            lock (_streams) _streams.Add(s);
            StreamOpened?.Invoke(format.lindex, s);
            medium.tymed = TYMED.TYMED_ISTREAM;
            medium.unionmember = Marshal.GetComInterfaceForObject(s, typeof(IStream));
            return;
        }
        byte[] bytes;
        if (Entry(format.cfFormat) is { } entry)
        {
            try { bytes = entry.Render(); }
            catch (Exception e)
            {
                RenderFailed?.Invoke(entry.Format, e);
                throw new COMException("delayed render failed", E_FAIL);
            }
        }
        else if (format.cfFormat == _cfDescriptor) bytes = ShellFormats.FileGroupDescriptor(_files);
        else if (format.cfFormat == _cfDropEffect) bytes = ShellFormats.Dword(ShellFormats.DROPEFFECT_COPY);
        else if (format.cfFormat == _cfOrigin) bytes = ShellFormats.Dword(1);
        else bytes = Received(format.cfFormat) ?? [];
        medium.tymed = TYMED.TYMED_HGLOBAL;
        medium.unionmember = ShellFormats.ToHGlobal(bytes);
    }

    public int QueryGetData(ref FORMATETC format)
    {
        if (format.dwAspect != DVASPECT.DVASPECT_CONTENT) return DV_E_FORMATETC;
        var cf = format.cfFormat;
        if (_files.Count > 0 && cf == _cfContents)
        {
            if (format.lindex < 0 || format.lindex >= _files.Count || _files[format.lindex].IsDirectory) return DV_E_LINDEX;
            return (format.tymed & TYMED.TYMED_ISTREAM) != 0 ? S_OK : DV_E_TYMED;
        }
        var ours = Entry(cf) is not null || cf == _cfOrigin || (_files.Count > 0 && (cf == _cfDescriptor || cf == _cfDropEffect));
        if (ours || Received(cf) is not null) return (format.tymed & TYMED.TYMED_HGLOBAL) != 0 ? S_OK : DV_E_TYMED;
        return DV_E_FORMATETC;
    }

    public IEnumFORMATETC EnumFormatEtc(DATADIR direction)
    {
        if (direction != DATADIR.DATADIR_GET) Marshal.ThrowExceptionForHR(E_NOTIMPL);
        var list = new List<FORMATETC>();
        void Add(short cf, TYMED t) => list.Add(new FORMATETC { cfFormat = cf, dwAspect = DVASPECT.DVASPECT_CONTENT, lindex = -1, tymed = t });
        foreach (var e in _entries) Add(unchecked((short)e.Format), TYMED.TYMED_HGLOBAL);
        if (_files.Count > 0)
        {
            Add(_cfDescriptor, TYMED.TYMED_HGLOBAL);
            Add(_cfContents, TYMED.TYMED_ISTREAM); // listed once with lindex -1; GetData gets the index
            Add(_cfDropEffect, TYMED.TYMED_HGLOBAL);
        }
        Add(_cfOrigin, TYMED.TYMED_HGLOBAL);
        return new FormatEnumerator([.. list]);
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
}
