namespace Fsync.Interop.Clipboard;

/// <summary>One clipboard format as seen by the listener: id, name, HGLOBAL size (no content).</summary>
public readonly record struct ClipFormat(uint Id, string Name, long Size);

public enum SkipReason
{
    None, OwnEcho, ExcludeFromMonitor, ViewerIgnore, NoClipboardHistory, NoCloudUpload,
    /// <summary>The owning process is on the user's denylist.</summary>
    DeniedApp,
    /// <summary>The clipboard stayed locked by another process.</summary>
    Unreadable,
    /// <summary>Nothing we can sync (no text, rich text, image or files).</summary>
    NoContent,
    /// <summary>Larger than the clip size limit.</summary>
    TooLarge,
}

/// <summary>
/// Decides whether a clipboard change may be synced. Reads only format names and the tiny
/// DWORD flag formats, never content (log names + sizes only).
/// </summary>
public static class SensitiveFormatFilter
{
    /// <summary>Pure decision over a format list; <paramref name="readDword"/> reads a flag format.</summary>
    public static SkipReason Classify(IReadOnlyList<ClipFormat> formats, Func<uint, uint?> readDword)
    {
        foreach (var f in formats)
        {
            switch (f.Name)
            {
                case ClipboardFormats.FsyncOrigin: return SkipReason.OwnEcho;
                case ClipboardFormats.ExcludeFromMonitor: return SkipReason.ExcludeFromMonitor;
                case ClipboardFormats.ViewerIgnore: return SkipReason.ViewerIgnore;
                case ClipboardFormats.CanIncludeInHistory when readDword(f.Id) == 0: return SkipReason.NoClipboardHistory;
                case ClipboardFormats.CanUploadToCloud when readDword(f.Id) == 0: return SkipReason.NoCloudUpload;
            }
        }
        return SkipReason.None;
    }

    /// <summary>
    /// True when <paramref name="process"/> (exe name, with or without ".exe") matches a denylist
    /// entry. Case-insensitive; entries may also carry the ".exe" suffix.
    /// </summary>
    public static bool IsDenied(string? process, IEnumerable<string> denylist)
    {
        if (string.IsNullOrWhiteSpace(process)) return false;
        var p = StripExe(process.Trim());
        return denylist.Any(d => !string.IsNullOrWhiteSpace(d) && string.Equals(StripExe(d.Trim()), p, StringComparison.OrdinalIgnoreCase));
    }

    private static string StripExe(string s) => s.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ? s[..^4] : s;

    /// <summary>Password managers skipped by default (exe names). The user can edit the list.</summary>
    public static readonly string[] DefaultDenylist = ["1Password", "KeePass", "KeePassXC", "Bitwarden", "Dashlane", "LastPass", "Enpass", "NordPass", "ProtonPass"];
}
