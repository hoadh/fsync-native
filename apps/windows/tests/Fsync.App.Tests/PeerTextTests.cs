using Fsync.App.Services;

namespace Fsync.App.Tests;

/// <summary>Spec section 5 vectors (shared with macOS PeerText).</summary>
public sealed class PeerTextTests
{
    private static readonly string Thumb = "\U0001F44D\U0001F3FD"; // 👍🏽: one cluster, two scalars

    public static TheoryData<string, string> Vectors => new()
    {
        { "Alice\nBob", "AliceBob" },
        { "‮evil", "evil" },
        { "gp‮j.exe", "gpj.exe" },
        { "  Mac  ", "Mac" },
        { "a‍b​c", "abc" },
        { new string('a', 40), new string('a', 40) },
        { new string('a', 41), new string('a', 40) + "…" },
        { new string('a', 39) + " b", new string('a', 39) + " …" },
        { "\u0007​\n", "" },
        { "a  b", "a  b" },
        { string.Concat(Enumerable.Repeat(Thumb, 41)), string.Concat(Enumerable.Repeat(Thumb, 40)) + "…" },
    };

    [Theory]
    [MemberData(nameof(Vectors))]
    public void SanitizeMatchesTheSpecVectors(string input, string expected) => Assert.Equal(expected, PeerText.Sanitize(input));

    [Fact]
    public void EmptyFallsBackToAnotherDevice()
    {
        Assert.Equal("", PeerText.Sanitize(null));
        Assert.Equal("another device", PeerText.DisplayName(null));
        Assert.Equal("another device", PeerText.DisplayName("\u0007​\n"));
        Assert.Equal("another device", PeerText.DisplayName("   "));
        Assert.Equal("Mac", PeerText.DisplayName(" Mac\t"));
    }

    [Fact]
    public void OtherWhitespaceAndLimits()
    {
        Assert.Equal("Mac", PeerText.Sanitize(" Mac　")); // Zs separators are trimmed
        Assert.Equal("ab…", PeerText.Sanitize("abc", limit: 2));
        Assert.Throws<ArgumentOutOfRangeException>(() => PeerText.Sanitize("a", -1));
    }
}
