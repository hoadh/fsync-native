using System.Text;
using Fsync.Core;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Imaging;

namespace Fsync.App.Clipboard;

/// <summary>
/// Windows clipboard formats ↔ wire MIME types.
/// Out: CF_HDROP → files; "HTML Format" (unwrapped) → text/html; "Rich Text Format" → text/rtf;
/// CF_UNICODETEXT → text/plain; "PNG" or CF_DIB(V5) → image/png.
/// Rep order decides the clip kind in core (first rep): rich formats first, then text, then image.
/// </summary>
public static class RepresentationMapper
{
    public const string TextPlain = "text/plain", TextHtml = "text/html", TextRtf = "text/rtf", ImagePng = "image/png";

    /// <summary>The clip to publish for a snapshot, or null when nothing is syncable.</summary>
    public static async Task<LocalClip?> ToLocalClipAsync(ClipboardSnapshot s)
    {
        if (s.Skip != SkipReason.None) return null;
        if (s.Files.Count > 0)
        {
            var existing = s.Files.Where(p => File.Exists(p) || Directory.Exists(p)).ToArray();
            return existing.Length == 0 ? null : new LocalClip([], existing, Preview(existing), sensitive: false);
        }
        var reps = new List<Rep>();
        if (s.Html is { } cfHtml && CfHtmlCodec.Decode(cfHtml) is { Length: > 0 } html)
            reps.Add(new Rep(TextHtml, Encoding.UTF8.GetBytes(html)));
        if (s.Rtf is { } rtf) reps.Add(new Rep(TextRtf, TrimNul(rtf)));
        if (s.Text is { Length: > 0 } text) reps.Add(new Rep(TextPlain, Encoding.UTF8.GetBytes(text)));
        var png = s.Png ?? (s.Dib is { } dib ? await TryDibToPngAsync(dib) : null);
        if (png is not null) reps.Add(new Rep(ImagePng, png));
        if (reps.Count == 0) return null;
        var preview = s.Text is { Length: > 0 } t ? Cut(t) : png is not null ? "Image" : null;
        return new LocalClip([.. reps], [], preview, sensitive: false);
    }

    private static async Task<byte[]?> TryDibToPngAsync(byte[] dib)
    {
        try { return await ImageCodec.DibToPngAsync(dib); }
        catch (Exception e) when (e is InvalidDataException or ArgumentException or System.Runtime.InteropServices.COMException)
        {
            return null; // a DIB WIC cannot read: sync the other formats only
        }
    }

    /// <summary>"report.pdf" or "report.pdf and 2 more" (names only, shown in history).</summary>
    public static string Preview(IReadOnlyList<string> paths)
    {
        var first = Path.GetFileName(paths[0].TrimEnd('\\', '/'));
        return paths.Count == 1 ? first : $"{first} and {paths.Count - 1} more";
    }

    private static string Cut(string s) => s.Length <= 200 ? s : s[..200];

    /// <summary>RTF/HGLOBAL payloads may carry trailing NULs (HGLOBAL rounding).</summary>
    internal static byte[] TrimNul(byte[] b)
    {
        var end = b.Length;
        while (end > 0 && b[end - 1] == 0) end--;
        return end == b.Length ? b : b[..end];
    }

    /// <summary>
    /// The clipboard formats for one received representation. Images are written both as "PNG"
    /// (browsers, Office) and as CF_DIB (everything else). Unknown MIME types: none.
    /// </summary>
    public static IReadOnlyList<(uint Format, Func<byte[], byte[]> Convert)> FormatsFor(string mime) => mime switch
    {
        TextPlain => [(ClipboardFormats.CF_UNICODETEXT, b => Encoding.Unicode.GetBytes(Encoding.UTF8.GetString(b) + "\0"))],
        TextHtml => [(ClipboardFormats.Register(ClipboardFormats.Html), b => CfHtmlCodec.Encode(Encoding.UTF8.GetString(b)))],
        TextRtf => [(ClipboardFormats.Register(ClipboardFormats.Rtf), b => b)],
        ImagePng =>
        [
            (ClipboardFormats.Register(ClipboardFormats.Png), b => b),
            (ClipboardFormats.CF_DIB, b => Task.Run(() => ImageCodec.PngToDibAsync(b)).GetAwaiter().GetResult()),
        ],
        _ => [],
    };
}
