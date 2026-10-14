using System.Text;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Shell;

namespace Fsync.Interop.Tests;

public sealed class CodecTests
{
    [Theory]
    [InlineData("<b>bold</b> text")]
    [InlineData("<p>Xin chào – 日本語 ✓</p>")]
    [InlineData("")]
    public void CfHtmlRoundTripsFragments(string fragment)
    {
        var bytes = CfHtmlCodec.Encode(fragment);
        Assert.Equal(0, bytes[^1]);
        Assert.Equal(fragment, CfHtmlCodec.Decode(bytes));
    }

    [Fact]
    public void CfHtmlHeaderOffsetsAreUtf8ByteOffsets()
    {
        var bytes = CfHtmlCodec.Encode("<i>é</i>");
        var text = Encoding.UTF8.GetString(bytes, 0, bytes.Length - 1);
        int Field(string key) => int.Parse(text.Split("\r\n").First(l => l.StartsWith(key))[key.Length..]);
        var start = Field("StartFragment:");
        var end = Field("EndFragment:");
        Assert.Equal("<i>é</i>", Encoding.UTF8.GetString(bytes, start, end - start));
        Assert.Equal(bytes.Length - 1, Field("EndHTML:"));
        Assert.StartsWith("<html>", Encoding.UTF8.GetString(bytes, Field("StartHTML:"), 6));
    }

    [Fact]
    public void CfHtmlKeepsTheBodyOfAFullDocument()
    {
        var doc = "<html><head><style>b{color:red}</style></head><body class=\"x\"><b>hi</b></body></html>";
        Assert.Equal("<b>hi</b>", CfHtmlCodec.Decode(CfHtmlCodec.Encode(doc)));
    }

    /// <summary>What Chrome / Word put on the clipboard: CRLF header, fragment comments, no NUL.</summary>
    [Fact]
    public void CfHtmlDecodesForeignWriters()
    {
        const string body = "<html>\r\n<body>\r\n<!--StartFragment--><span>Ä</span><!--EndFragment-->\r\n</body>\r\n</html>";
        var header = "Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:{0:D10}\r\nStartFragment:{1:D10}\r\nEndFragment:{2:D10}\r\n";
        var probe = string.Format(header, 0, 0, 0);
        var start = Encoding.UTF8.GetByteCount(probe + body[..body.IndexOf("<span", StringComparison.Ordinal)]);
        var end = start + Encoding.UTF8.GetByteCount("<span>Ä</span>");
        var all = string.Format(header, Encoding.UTF8.GetByteCount(probe + body), start, end) + body;
        Assert.Equal("<span>Ä</span>", CfHtmlCodec.Decode(Encoding.UTF8.GetBytes(all)));
        // Offsets missing: the comment markers still give the fragment.
        Assert.Equal("<span>Ä</span>", CfHtmlCodec.Decode(Encoding.UTF8.GetBytes("Version:0.9\r\n" + body)));
        Assert.Null(CfHtmlCodec.Decode(Encoding.UTF8.GetBytes("not html")));
    }

    [Fact]
    public void DropFilesRoundTrip()
    {
        string[] paths = [@"C:\a b\one.txt", @"D:\日本\two.bin"];
        Assert.Equal(paths, DropFiles.Parse(DropFiles.Build(paths)));
        Assert.Empty(DropFiles.Parse([1, 2, 3]));
    }

    [Fact]
    public void FolderEntriesComeBeforeTheirFilesAndUnsafePathsAreDropped()
    {
        var t = DateTime.UtcNow;
        var list = VirtualFile.WithFolders([
            new("a.txt", 1, t, null), new("dir/sub/b.txt", 2, t, null), new("dir/c.txt", 3, t, null),
            new("../evil.txt", 4, t, null), new(@"C:\abs.txt", 5, t, null), new("dir/con?.txt", 6, t, null),
        ]);
        Assert.Equal(["a.txt", "dir", @"dir\sub", @"dir\sub\b.txt", @"dir\c.txt"], list.Select(f => f.RelativePath));
        Assert.True(list[1].IsDirectory);
        Assert.Equal(list.Select(f => f.RelativePath), ShellFormats.DescriptorNames(ShellFormats.FileGroupDescriptor(list)));
    }

    [Fact]
    public void DenylistMatchesExeNamesCaseInsensitively()
    {
        string[] deny = ["KeePass", "1password.exe"];
        Assert.True(SensitiveFormatFilter.IsDenied("keepass", deny));
        Assert.True(SensitiveFormatFilter.IsDenied("1Password", deny));
        Assert.True(SensitiveFormatFilter.IsDenied("KeePass.exe", deny));
        Assert.False(SensitiveFormatFilter.IsDenied("notepad", deny));
        Assert.False(SensitiveFormatFilter.IsDenied(null, deny));
    }
}
