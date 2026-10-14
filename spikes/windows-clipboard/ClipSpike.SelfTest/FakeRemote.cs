using System.Security.Cryptography;

namespace ClipSpike.SelfTest;

/// <summary>
/// Deterministic "remote" file contents with simulated per-request latency, standing in for
/// core's read_file_range. Byte i of file <c>seed</c> = (i * 31 + seed) mod 256.
/// </summary>
internal static class FakeRemote
{
    public static VirtualFile File(string name, long length, byte seed, TimeSpan latency) =>
        new(name, length, new DateTime(2026, 10, 14, 12, 0, 0, DateTimeKind.Utc), (offset, len) =>
        {
            if (latency > TimeSpan.Zero) Thread.Sleep(latency);
            return Bytes(offset, len, seed);
        });

    public static byte[] Bytes(long offset, int len, byte seed)
    {
        var b = new byte[len];
        for (var i = 0; i < len; i++) b[i] = unchecked((byte)((offset + i) * 31 + seed));
        return b;
    }

    /// <summary>SHA-256 of the whole synthetic file, computed in 1 MiB steps.</summary>
    public static string Hash(long length, byte seed)
    {
        using var h = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        for (long off = 0; off < length; off += 1 << 20)
            h.AppendData(Bytes(off, (int)Math.Min(1 << 20, length - off), seed));
        return Convert.ToHexString(h.GetHashAndReset());
    }

    public static string HashFile(string path)
    {
        using var s = System.IO.File.OpenRead(path);
        return Convert.ToHexString(SHA256.HashData(s));
    }
}
