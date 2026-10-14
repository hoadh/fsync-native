using System.Text;

namespace Fsync.Interop.Clipboard;

/// <summary>CF_HDROP payload (DROPFILES + double-NUL-terminated path list) build and parse.</summary>
public static class DropFiles
{
    private const int HeaderSize = 20; // pFiles 4, pt 8, fNC 4, fWide 4

    public static byte[] Build(IEnumerable<string> paths)
    {
        var list = string.Concat(paths.Select(p => p + "\0")) + "\0";
        var body = Encoding.Unicode.GetBytes(list);
        var buf = new byte[HeaderSize + body.Length];
        BitConverter.GetBytes(HeaderSize).CopyTo(buf, 0);
        BitConverter.GetBytes(1).CopyTo(buf, 16); // fWide
        body.CopyTo(buf, HeaderSize);
        return buf;
    }

    /// <summary>Paths in a CF_HDROP payload; empty when malformed.</summary>
    public static List<string> Parse(byte[] data)
    {
        var result = new List<string>();
        if (data.Length < HeaderSize) return result;
        var offset = BitConverter.ToInt32(data, 0);
        var wide = BitConverter.ToInt32(data, 16) != 0;
        if (offset < HeaderSize || offset >= data.Length) return result;
        var text = wide ? Encoding.Unicode.GetString(data, offset, (data.Length - offset) & ~1)
                        : Encoding.Default.GetString(data, offset, data.Length - offset);
        foreach (var part in text.Split('\0'))
        {
            if (part.Length == 0) break; // the empty entry before the final NUL ends the list
            result.Add(part);
        }
        return result;
    }
}
