using Windows.Win32;

namespace Fsync.Interop.Clipboard;

/// <summary>Clipboard format ids and names we read, write or check (registered names resolve per session).</summary>
public static unsafe class ClipboardFormats
{
    public const uint CF_UNICODETEXT = 13, CF_HDROP = 15, CF_DIB = 8, CF_DIBV5 = 17;

    public const string Html = "HTML Format";
    public const string Rtf = "Rich Text Format";
    public const string Png = "PNG";

    // Skip rules: https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats
    public const string ExcludeFromMonitor = "ExcludeClipboardContentFromMonitorProcessing";
    public const string CanIncludeInHistory = "CanIncludeInClipboardHistory";
    public const string CanUploadToCloud = "CanUploadToCloudClipboard";
    /// <summary>Older convention (KeePass and others).</summary>
    public const string ViewerIgnore = "Clipboard Viewer Ignore";
    /// <summary>Marker on everything fsync puts on the clipboard (echo guard).</summary>
    public const string FsyncOrigin = "FsyncOrigin";

    public static uint Register(string name)
    {
        fixed (char* p = name) return PInvoke.RegisterClipboardFormat(p);
    }

    /// <summary>Registered name, or a CF_* name for standard formats.</summary>
    public static string Name(uint format)
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

    /// <summary>GDI-handle formats: they have no HGLOBAL, so no size and no bytes.</summary>
    public static bool IsGdiHandle(uint f) => f is 2 or 3 or 9 or 14;
}
