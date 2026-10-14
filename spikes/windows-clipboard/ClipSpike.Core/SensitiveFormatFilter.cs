using Windows.Win32;
using Windows.Win32.Foundation;

namespace ClipSpike;

/// <summary>One clipboard format as seen by the listener: id, name, HGLOBAL size (no content).</summary>
public readonly record struct ClipFormat(uint Id, string Name, long Size);

public enum SkipReason { None, ExcludeFromMonitor, NoClipboardHistory, NoCloudUpload, ViewerIgnore, OwnEcho, Unreadable }

/// <summary>
/// Decides whether a clipboard change may be synced. Reads only format names and the tiny
/// DWORD flag formats, never content (security rule: log names + sizes only).
/// </summary>
public static unsafe class SensitiveFormatFilter
{
    // https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats
    public const string ExcludeFromMonitor = "ExcludeClipboardContentFromMonitorProcessing";
    public const string CanIncludeInHistory = "CanIncludeInClipboardHistory";
    public const string CanUploadToCloud = "CanUploadToCloudClipboard";
    /// <summary>Older convention (KeePass, some others).</summary>
    public const string ViewerIgnore = "Clipboard Viewer Ignore";
    /// <summary>Self-write marker set on everything we put on the clipboard.</summary>
    public const string FsyncOrigin = "FsyncOrigin";

    public static uint Register(string name)
    {
        fixed (char* p = name) return PInvoke.RegisterClipboardFormat(p);
    }

    /// <summary>Pure decision over a format list; <paramref name="readDword"/> reads a flag format.</summary>
    public static SkipReason Classify(IReadOnlyList<ClipFormat>? formats, Func<uint, uint?> readDword)
    {
        if (formats is null) return SkipReason.Unreadable;
        foreach (var f in formats)
        {
            switch (f.Name)
            {
                case FsyncOrigin: return SkipReason.OwnEcho;
                case ExcludeFromMonitor: return SkipReason.ExcludeFromMonitor;
                case ViewerIgnore: return SkipReason.ViewerIgnore;
                case CanIncludeInHistory when readDword(f.Id) == 0: return SkipReason.NoClipboardHistory;
                case CanUploadToCloud when readDword(f.Id) == 0: return SkipReason.NoCloudUpload;
            }
        }
        return SkipReason.None;
    }

    /// <summary>Format snapshot + decision in one clipboard open (null formats: could not open).</summary>
    public static (List<ClipFormat>? Formats, SkipReason Reason) Inspect()
    {
        for (var i = 0; !PInvoke.OpenClipboard(default); i++)
        {
            if (i >= 5) return (null, SkipReason.Unreadable);
            Thread.Sleep(20); // another process holds it; documented to be brief
        }
        try
        {
            var list = new List<ClipFormat>();
            for (uint f = PInvoke.EnumClipboardFormats(0); f != 0; f = PInvoke.EnumClipboardFormats(f))
                list.Add(new ClipFormat(f, FormatName(f), HandleSize(f)));
            return (list, Classify(list, ReadDword));
        }
        finally
        {
            PInvoke.CloseClipboard();
        }
    }

    private static uint? ReadDword(uint format)
    {
        var h = PInvoke.GetClipboardData(format);
        if (h.IsNull) return null;
        var g = (HGLOBAL)h.Value;
        if ((long)(nuint)PInvoke.GlobalSize(g) < 4) return null;
        var p = PInvoke.GlobalLock(g);
        if (p is null) return null;
        try { return *(uint*)p; } finally { PInvoke.GlobalUnlock(g); }
    }

    private static long HandleSize(uint format)
    {
        // Only HGLOBAL-backed formats have a size; GDI handles (bitmaps, palettes) do not.
        if (format is 2 or 3 or 9 or 14) return -1; // CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE
        var h = PInvoke.GetClipboardData(format);
        return h.IsNull ? -1 : (long)(nuint)PInvoke.GlobalSize((HGLOBAL)h.Value);
    }

    public static string FormatName(uint format)
    {
        var buf = stackalloc char[256];
        var n = PInvoke.GetClipboardFormatName(format, buf, 256);
        return n > 0 ? new string(buf, 0, n) : StandardName(format);
    }

    private static string StandardName(uint f) => f switch
    {
        1 => "CF_TEXT", 2 => "CF_BITMAP", 3 => "CF_METAFILEPICT", 7 => "CF_OEMTEXT", 8 => "CF_DIB",
        9 => "CF_PALETTE", 13 => "CF_UNICODETEXT", 14 => "CF_ENHMETAFILE", 15 => "CF_HDROP",
        16 => "CF_LOCALE", 17 => "CF_DIBV5", _ => $"#{f}",
    };
}
