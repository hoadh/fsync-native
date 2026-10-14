using System.Globalization;
using System.Text;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// "HTML Format" (CF_HTML) wrap/unwrap. The payload is UTF-8 with a header of byte offsets:
/// https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format
/// The wire carries the plain HTML fragment (`text/html`).
/// </summary>
public static class CfHtmlCodec
{
    private const string StartMarker = "<!--StartFragment-->", EndMarker = "<!--EndFragment-->";
    private const string HeaderTemplate = "Version:0.9\r\nStartHTML:{0:D10}\r\nEndHTML:{1:D10}\r\nStartFragment:{2:D10}\r\nEndFragment:{3:D10}\r\n";

    /// <summary>Wraps an HTML fragment (or a full document) into CF_HTML bytes, NUL-terminated.</summary>
    public static byte[] Encode(string html)
    {
        string prefix, suffix;
        if (html.Contains("<body", StringComparison.OrdinalIgnoreCase))
        {
            // A full document: the fragment is everything inside <body>.
            var bodyOpen = html.IndexOf('>', html.IndexOf("<body", StringComparison.OrdinalIgnoreCase)) + 1;
            var bodyClose = html.LastIndexOf("</body", StringComparison.OrdinalIgnoreCase);
            if (bodyClose < bodyOpen) bodyClose = html.Length;
            prefix = html[..bodyOpen] + StartMarker;
            suffix = EndMarker + html[bodyClose..];
            html = html[bodyOpen..bodyClose];
        }
        else
        {
            prefix = "<html><body>" + StartMarker;
            suffix = EndMarker + "</body></html>";
        }
        var headerLen = Encoding.UTF8.GetByteCount(string.Format(CultureInfo.InvariantCulture, HeaderTemplate, 0, 0, 0, 0));
        var startFrag = headerLen + Encoding.UTF8.GetByteCount(prefix);
        var endFrag = startFrag + Encoding.UTF8.GetByteCount(html);
        var endHtml = endFrag + Encoding.UTF8.GetByteCount(suffix);
        var header = string.Format(CultureInfo.InvariantCulture, HeaderTemplate, headerLen, endHtml, startFrag, endFrag);
        return [.. Encoding.UTF8.GetBytes(header + prefix + html + suffix), 0];
    }

    /// <summary>The fragment from CF_HTML bytes, or null when the header is missing or broken.</summary>
    public static string? Decode(byte[] data)
    {
        var len = Array.IndexOf(data, (byte)0);
        if (len < 0) len = data.Length;
        // The header is ASCII, so byte offsets can be read from a Latin-1 view.
        var head = Encoding.Latin1.GetString(data, 0, Math.Min(len, 512));
        var start = Offset(head, "StartFragment:");
        var end = Offset(head, "EndFragment:");
        if (start is { } s && end is { } e && s >= 0 && e >= s && e <= len)
            return Encoding.UTF8.GetString(data, s, e - s);
        // Some writers omit fragment offsets: fall back to the HTML range or the markers.
        var all = Encoding.UTF8.GetString(data, 0, len);
        var a = all.IndexOf(StartMarker, StringComparison.Ordinal);
        var b = all.IndexOf(EndMarker, StringComparison.Ordinal);
        if (a >= 0 && b > a) return all[(a + StartMarker.Length)..b];
        var sh = Offset(head, "StartHTML:");
        return sh is { } h && h >= 0 && h <= len ? Encoding.UTF8.GetString(data, h, len - h) : null;
    }

    private static int? Offset(string head, string key)
    {
        var i = head.IndexOf(key, StringComparison.Ordinal);
        if (i < 0) return null;
        i += key.Length;
        var j = i;
        while (j < head.Length && (char.IsDigit(head[j]) || head[j] == '-')) j++;
        return int.TryParse(head.AsSpan(i, j - i), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var v) ? v : null;
    }
}
