using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using STATSTG = System.Runtime.InteropServices.ComTypes.STATSTG;

namespace ClipSpike;

/// <summary>Reads <c>len</c> bytes at <c>offset</c> (core: <c>read_file_range</c>, blocking, 30 s deadline).</summary>
public delegate byte[] RangeReader(long offset, int len);

/// <summary>
/// Read-only, seekable IStream over a remote file. Explorer calls Read on its own (copy engine)
/// thread; each Read becomes one range fetch, capped at <see cref="MaxChunk"/>.
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
    private long _pos;

    public RemoteFileStream(string name, long length, RangeReader reader)
    {
        _name = name;
        Length = length;
        _reader = reader;
    }

    public long Length { get; }
    /// <summary>Total bytes handed to the caller (spike metrics).</summary>
    public long BytesServed { get; private set; }
    public int Reads { get; private set; }
    private static int s_inFlight, s_maxInFlight;
    /// <summary>Most range reads seen in flight at once, across all streams (spike metric).</summary>
    public static int MaxConcurrentReads => Volatile.Read(ref s_maxInFlight);
    public static void ResetConcurrency() => Volatile.Write(ref s_maxInFlight, 0);
    /// <summary>Set by the owner to fail subsequent reads (cancel / app exit).</summary>
    public volatile bool Aborted;

    public void Read(byte[] pv, int cb, IntPtr pcbRead)
    {
        if (Aborted) throw new COMException("transfer cancelled", E_ABORT);
        var n = (int)Math.Min(Math.Min(cb, MaxChunk), Math.Max(0, Length - _pos));
        var got = 0;
        if (n > 0)
        {
            byte[] chunk;
            var now = Interlocked.Increment(ref s_inFlight);
            for (var max = s_maxInFlight; now > max; max = s_maxInFlight)
                Interlocked.CompareExchange(ref s_maxInFlight, now, max);
            try { chunk = _reader(_pos, n); }
            catch (Exception e) when (e is not COMException)
            {
                throw new COMException($"range read failed: {e.Message}", STG_E_READFAULT);
            }
            finally { Interlocked.Decrement(ref s_inFlight); }
            got = Math.Min(chunk.Length, n);
            Buffer.BlockCopy(chunk, 0, pv, 0, got);
            _pos += got;
            BytesServed += got;
            Reads++;
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

    public void Clone(out IStream ppstm)
    {
        var c = new RemoteFileStream(_name, Length, _reader) { _pos = _pos };
        ppstm = c;
    }

    public void Write(byte[] pv, int cb, IntPtr pcbWritten) => throw new COMException("read-only", STG_E_INVALIDFUNCTION);
    public void SetSize(long libNewSize) => throw new COMException("read-only", STG_E_INVALIDFUNCTION);
    public void CopyTo(IStream pstm, long cb, IntPtr pcbRead, IntPtr pcbWritten) => throw new NotImplementedException();
    public void Commit(int grfCommitFlags) { }
    public void Revert() { }
    public void LockRegion(long libOffset, long cb, int dwLockType) => throw new COMException("no locking", STG_E_INVALIDFUNCTION);
    public void UnlockRegion(long libOffset, long cb, int dwLockType) => throw new COMException("no locking", STG_E_INVALIDFUNCTION);
}
