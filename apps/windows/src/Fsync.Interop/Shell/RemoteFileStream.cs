using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using STATSTG = System.Runtime.InteropServices.ComTypes.STATSTG;

namespace Fsync.Interop.Shell;

/// <summary>
/// Read-only, seekable IStream over a remote file. Explorer calls Read on its own copy thread,
/// one 256 KiB chunk at a time; each Read is one range fetch (capped at <see cref="MaxChunk"/>).
/// Throughput is one chunk per round trip until core reads ahead (ADR 010 decision 6).
/// </summary>
[ComVisible(true)]
public sealed class RemoteFileStream : IStream
{
    public const int MaxChunk = 1 << 20; // 1 MiB: ~1.3 ms FFI copy each way (phase 03 spike)
    private const int STG_E_INVALIDFUNCTION = unchecked((int)0x80030001);
    private const int STG_E_READFAULT = unchecked((int)0x8003001E);
    private const int E_ABORT = unchecked((int)0x80004004);

    private readonly RangeReader _reader;
    private readonly string _name;
    private readonly Action<long>? _progress;
    private long _pos;

    /// <param name="progress">Called after each read with the bytes served so far.</param>
    public RemoteFileStream(string name, long length, RangeReader reader, Action<long>? progress = null)
    {
        _name = name;
        Length = length;
        _reader = reader;
        _progress = progress;
    }

    public long Length { get; }
    public long BytesServed { get; private set; }
    /// <summary>Set by the owner to fail later reads (user cancel, app exit).</summary>
    public volatile bool Aborted;

    public void Read(byte[] pv, int cb, IntPtr pcbRead)
    {
        if (Aborted) throw new COMException("transfer cancelled", E_ABORT);
        var n = (int)Math.Min(Math.Min(cb, MaxChunk), Math.Max(0, Length - _pos));
        var got = 0;
        if (n > 0)
        {
            byte[] chunk;
            try { chunk = _reader(_pos, n); }
            catch (Exception e) when (e is not COMException)
            {
                // The shell shows "Error Copying File or Folder" and deletes the partial file.
                throw new COMException("range read failed", STG_E_READFAULT);
            }
            got = Math.Min(chunk.Length, n);
            Buffer.BlockCopy(chunk, 0, pv, 0, got);
            _pos += got;
            BytesServed += got;
            _progress?.Invoke(BytesServed);
        }
        if (pcbRead != IntPtr.Zero) Marshal.WriteInt32(pcbRead, got);
    }

    public void Seek(long dlibMove, int dwOrigin, IntPtr plibNewPosition)
    {
        var basePos = dwOrigin switch { 0 => 0, 1 => _pos, 2 => Length, _ => throw new COMException("bad origin", STG_E_INVALIDFUNCTION) };
        var next = basePos + dlibMove;
        if (next < 0) throw new COMException("seek before start", STG_E_INVALIDFUNCTION);
        _pos = next;
        if (plibNewPosition != IntPtr.Zero) Marshal.WriteInt64(plibNewPosition, _pos);
    }

    public void Stat(out STATSTG pstatstg, int grfStatFlag)
    {
        pstatstg = new STATSTG
        {
            pwcsName = (grfStatFlag & 1) != 0 ? null! : _name, // STATFLAG_NONAME
            type = 2, // STGTY_STREAM
            cbSize = Length,
        };
    }

    public void Clone(out IStream ppstm) => ppstm = new RemoteFileStream(_name, Length, _reader, _progress) { _pos = _pos };

    public void Write(byte[] pv, int cb, IntPtr pcbWritten) => throw new COMException("read-only", STG_E_INVALIDFUNCTION);
    public void SetSize(long libNewSize) => throw new COMException("read-only", STG_E_INVALIDFUNCTION);
    public void CopyTo(IStream pstm, long cb, IntPtr pcbRead, IntPtr pcbWritten) => throw new COMException("not supported", STG_E_INVALIDFUNCTION);
    public void Commit(int grfCommitFlags) { }
    public void Revert() { }
    public void LockRegion(long libOffset, long cb, int dwLockType) => throw new COMException("no locking", STG_E_INVALIDFUNCTION);
    public void UnlockRegion(long libOffset, long cb, int dwLockType) => throw new COMException("no locking", STG_E_INVALIDFUNCTION);
}
