using System.Globalization;
using System.Text;

namespace Fsync.App.Services;

/// <summary>
/// Names that come from another device are untrusted text. Port of macOS <c>PeerText.sanitize</c>
/// (spec section 5): drop Cc/Cf scalars, trim space separators, cap at <c>limit</c> text elements + "…".
/// </summary>
public static class PeerText
{
    /// <summary>Shown when a peer name is missing or sanitizes to nothing.</summary>
    public const string Fallback = "another device";

    /// <summary>Sanitized text, or "" when nothing is left (callers use <see cref="DisplayName"/> for the fallback).</summary>
    public static string Sanitize(string? text, int limit = 40)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(limit);
        if (string.IsNullOrEmpty(text)) return "";
        var sb = new StringBuilder(text.Length);
        foreach (var rune in text.EnumerateRunes())
        {
            var cat = Rune.GetUnicodeCategory(rune);
            if (cat is UnicodeCategory.Control or UnicodeCategory.Format) continue;
            sb.Append(rune.ToString());
        }
        var cleaned = TrimSpaces(sb.ToString());
        // Length counts grapheme clusters (StringInfo text elements), not UTF-16 units.
        var info = new StringInfo(cleaned);
        return info.LengthInTextElements <= limit ? cleaned : info.SubstringByTextElements(0, limit) + "…";
    }

    /// <summary>Sanitized name, or <see cref="Fallback"/> when it is empty.</summary>
    public static string DisplayName(string? text, int limit = 40)
    {
        var name = Sanitize(text, limit);
        return name.Length == 0 ? Fallback : name;
    }

    /// <summary>Swift <c>.whitespaces</c>: space separators (Zs) only; tabs are already removed as Cc.</summary>
    private static string TrimSpaces(string s)
    {
        int start = 0, end = s.Length;
        while (start < end && char.GetUnicodeCategory(s[start]) == UnicodeCategory.SpaceSeparator) start++;
        while (end > start && char.GetUnicodeCategory(s[end - 1]) == UnicodeCategory.SpaceSeparator) end--;
        return s[start..end];
    }
}
