using System.Runtime.InteropServices;
using System.Text;

namespace Fsync.Interop.Shell;

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
    private const uint FILE_ATTRIBUTE_NORMAL = 0x80, FILE_ATTRIBUTE_DIRECTORY = 0x10;
    // FILEDESCRIPTORW layout (x64 and ARM64 identical, no padding): flags 4, clsid 16, sizel 8,
    // pointl 8, attributes 4, 3 x FILETIME 24, size hi/lo 8, cFileName 260 WCHAR.
    internal const int DescriptorSize = 4 + 16 + 8 + 8 + 4 + 24 + 8 + 520;
    internal const int OffAttributes = 36, OffWriteTime = 56, OffSizeHigh = 64, OffName = 72;

    /// <summary>FILEGROUPDESCRIPTORW bytes for <paramref name="files"/> (order = lindex).</summary>
    public static byte[] FileGroupDescriptor(IReadOnlyList<VirtualFile> files)
    {
        var buf = new byte[4 + DescriptorSize * files.Count];
        BitConverter.GetBytes((uint)files.Count).CopyTo(buf, 0);
        for (var i = 0; i < files.Count; i++)
        {
            var f = files[i];
            var o = 4 + i * DescriptorSize;
            var flags = FD_ATTRIBUTES | FD_WRITESTIME | FD_PROGRESSUI | FD_UNICODE | (f.IsDirectory ? 0 : FD_FILESIZE);
            BitConverter.GetBytes(flags).CopyTo(buf, o);
            BitConverter.GetBytes(f.IsDirectory ? FILE_ATTRIBUTE_DIRECTORY : FILE_ATTRIBUTE_NORMAL).CopyTo(buf, o + OffAttributes);
            BitConverter.GetBytes(f.Modified.ToFileTimeUtc()).CopyTo(buf, o + OffWriteTime);
            BitConverter.GetBytes((uint)(f.Length >> 32)).CopyTo(buf, o + OffSizeHigh);
            BitConverter.GetBytes((uint)(f.Length & 0xFFFFFFFF)).CopyTo(buf, o + OffSizeHigh + 4);
            var name = f.RelativePath.Length > 259 ? f.RelativePath[..259] : f.RelativePath;
            Encoding.Unicode.GetBytes(name).CopyTo(buf, o + OffName);
        }
        return buf;
    }

    /// <summary>Names in a FILEGROUPDESCRIPTORW (read side, used by tests and diagnostics).</summary>
    public static List<string> DescriptorNames(byte[] data)
    {
        var names = new List<string>();
        if (data.Length < 4) return names;
        var count = BitConverter.ToUInt32(data, 0);
        for (var i = 0; i < count && 4 + (i + 1) * DescriptorSize <= data.Length; i++)
        {
            var s = Encoding.Unicode.GetString(data, 4 + i * DescriptorSize + OffName, 520);
            var end = s.IndexOf('\0');
            names.Add(end >= 0 ? s[..end] : s);
        }
        return names;
    }

    public static byte[] Dword(uint value) => BitConverter.GetBytes(value);

    /// <summary>Copies bytes into a fresh HGLOBAL (GMEM_FIXED); the receiver of the STGMEDIUM frees it.</summary>
    public static IntPtr ToHGlobal(byte[] bytes)
    {
        var h = Marshal.AllocHGlobal(Math.Max(bytes.Length, 1));
        Marshal.Copy(bytes, 0, h, bytes.Length);
        return h;
    }
}
