using System.Text;

namespace Fsync.Interop.Clipboard;

/// <summary>What was on the clipboard at one moment: the skip decision and the syncable content.</summary>
public sealed class ClipboardSnapshot
{
    public required IReadOnlyList<ClipFormat> Formats { get; init; }
    public SkipReason Skip { get; init; }
    public string? OwnerProcess { get; init; }
    public string? Text { get; init; }
    /// <summary>Raw CF_HTML bytes (decode with <see cref="CfHtmlCodec"/>).</summary>
    public byte[]? Html { get; init; }
    public byte[]? Rtf { get; init; }
    public byte[]? Png { get; init; }
    /// <summary>CF_DIBV5 or CF_DIB bytes when no PNG was offered.</summary>
    public byte[]? Dib { get; init; }
    public IReadOnlyList<string> Files { get; init; } = [];

    /// <summary>Only names and sizes: safe to log.</summary>
    public string Describe() => $"skip={Skip} owner={OwnerProcess ?? "?"} formats=[{string.Join(", ", Formats.Select(f => $"{f.Name}:{f.Size}"))}]";
}

/// <summary>Reads the syncable formats in one clipboard open, after the skip checks.</summary>
public static class ClipboardReader
{
    private static readonly HashSet<string> Wanted =
        ["CF_UNICODETEXT", "CF_HDROP", "CF_DIB", "CF_DIBV5", ClipboardFormats.Png, ClipboardFormats.Html, ClipboardFormats.Rtf];

    /// <param name="maxBytes">Formats larger than this are not read; if nothing else is left the
    /// snapshot says <see cref="SkipReason.TooLarge"/>.</param>
    /// <param name="denylist">Exe names whose clipboard writes are never synced.</param>
    public static ClipboardSnapshot Read(long maxBytes, IReadOnlyCollection<string> denylist)
    {
        var owner = ClipboardOwner.ProcessName();
        ClipboardSnapshot? snap = null;
        var opened = Win32Clipboard.TryWithOpen(() => snap = ReadOpen(maxBytes, owner, denylist));
        return opened && snap is not null ? snap : new ClipboardSnapshot { Formats = [], Skip = SkipReason.Unreadable, OwnerProcess = owner };
    }

    private static ClipboardSnapshot ReadOpen(long maxBytes, string? owner, IReadOnlyCollection<string> denylist)
    {
        // Skip checks first, on names and DWORD flags only: GetClipboardData forces a delayed
        // render, so sizes are taken afterwards and only for the formats we may read.
        var names = Win32Clipboard.Formats().Select(f => new ClipFormat(f, ClipboardFormats.Name(f), -1)).ToList();
        var skip = SensitiveFormatFilter.Classify(names, Win32Clipboard.ReadDword);
        if (skip == SkipReason.None && SensitiveFormatFilter.IsDenied(owner, denylist)) skip = SkipReason.DeniedApp;
        if (skip != SkipReason.None) return new ClipboardSnapshot { Formats = names, Skip = skip, OwnerProcess = owner };
        var formats = names.Select(f => Wanted.Contains(f.Name) ? f with { Size = Win32Clipboard.Size(f.Id) } : f).ToList();

        var tooLarge = false;
        byte[]? Get(string name)
        {
            var f = formats.FirstOrDefault(x => x.Name == name);
            if (f.Id == 0 || f.Size < 0) return null;
            if (f.Size > maxBytes) { tooLarge = true; return null; }
            return Win32Clipboard.Read(f.Id);
        }

        var text = Get("CF_UNICODETEXT") is { } t ? TrimNul(Encoding.Unicode.GetString(t)) : null;
        var files = Get("CF_HDROP") is { } d ? DropFiles.Parse(d) : [];
        var png = Get(ClipboardFormats.Png);
        var dib = png is null ? Get("CF_DIBV5") ?? Get("CF_DIB") : null;
        var snap = new ClipboardSnapshot
        {
            Formats = formats,
            OwnerProcess = owner,
            Text = text,
            Html = Get(ClipboardFormats.Html),
            Rtf = Get(ClipboardFormats.Rtf),
            Png = png,
            Dib = dib,
            Files = files,
        };
        var empty = text is null && files.Count == 0 && png is null && dib is null && snap.Html is null && snap.Rtf is null;
        return empty ? new ClipboardSnapshot { Formats = formats, OwnerProcess = owner, Skip = tooLarge ? SkipReason.TooLarge : SkipReason.NoContent } : snap;
    }

    private static string TrimNul(string s)
    {
        var i = s.IndexOf('\0');
        return i >= 0 ? s[..i] : s;
    }
}
