using System.Runtime.InteropServices;
using System.Text;

namespace ClipSpike;

/// <summary>Shell clipboard format names and HGLOBAL builders (FILEGROUPDESCRIPTORW, DWORDs).</summary>
public static class ShellFormats
{
    public const string FileDescriptorW = "FileGroupDescriptorW";
    public const string FileContents = "FileContents";
    public const string PreferredDropEffect = "Preferred DropEffect";
    public const string PerformedDropEffect = "Performed DropEffect";
    public const string PasteSucceeded = "Paste Succeeded";

    public const uint DROPEFFECT_COPY = 1;

    // FILEDESCRIPTORW flags
    private const uint FD_ATTRIBUTES = 0x4, FD_WRITESTIME = 0x20, FD_FILESIZE = 0x40, FD_PROGRESSUI = 0x4000, FD_UNICODE = 0x80000000;
    private const uint FILE_ATTRIBUTE_NORMAL = 0x80;
    // FILEDESCRIPTORW layout (x64 and ARM64 identical, no padding): flags 4, clsid 16, sizel 8,
    // pointl 8, attributes 4, 3 x FILETIME 24, size hi/lo 8, cFileName 260 WCHAR.
    private const int DescriptorSize = 4 + 16 + 8 + 8 + 4 + 24 + 8 + 520;
    private const int OffAttributes = 36, OffWriteTime = 56, OffSizeHigh = 64, OffName = 72;

    /// <summary>HGLOBAL with a FILEGROUPDESCRIPTORW; caller owns it (or hands it to STGMEDIUM).</summary>
    public static IntPtr FileGroupDescriptor(IReadOnlyList<VirtualFile> files)
    {
        var size = 4 + DescriptorSize * files.Count;
        var buf = new byte[size];
        BitConverter.GetBytes((uint)files.Count).CopyTo(buf, 0);
        for (var i = 0; i < files.Count; i++)
        {
            var f = files[i];
            var o = 4 + i * DescriptorSize;
            var flags = FD_ATTRIBUTES | FD_FILESIZE | FD_WRITESTIME | FD_PROGRESSUI | FD_UNICODE;
            BitConverter.GetBytes(flags).CopyTo(buf, o);
            BitConverter.GetBytes(FILE_ATTRIBUTE_NORMAL).CopyTo(buf, o + OffAttributes);
            BitConverter.GetBytes(f.Modified.ToFileTimeUtc()).CopyTo(buf, o + OffWriteTime);
            BitConverter.GetBytes((uint)(f.Length >> 32)).CopyTo(buf, o + OffSizeHigh);
            BitConverter.GetBytes((uint)(f.Length & 0xFFFFFFFF)).CopyTo(buf, o + OffSizeHigh + 4);
            var name = f.Name.Length > 259 ? f.Name[..259] : f.Name;
            Encoding.Unicode.GetBytes(name).CopyTo(buf, o + OffName);
        }
        return ToHGlobal(buf);
    }

    public static IntPtr Dword(uint value) => ToHGlobal(BitConverter.GetBytes(value));

    public static IntPtr ToHGlobal(byte[] bytes)
    {
        var h = Marshal.AllocHGlobal(Math.Max(bytes.Length, 1)); // GlobalAlloc(GMEM_FIXED)
        Marshal.Copy(bytes, 0, h, bytes.Length);
        return h;
    }

    /// <summary>Reads a DWORD from an HGLOBAL (Performed DropEffect, Paste Succeeded).</summary>
    public static uint? ReadDword(IntPtr hglobal) =>
        hglobal == IntPtr.Zero ? null : (uint)Marshal.ReadInt32(hglobal);
}

/// <summary>A remote file offered lazily: metadata now, bytes via <see cref="Reader"/> on paste.</summary>
public sealed record VirtualFile(string Name, long Length, DateTime Modified, RangeReader Reader);
