using System.Text;
using Fsync.Interop.Clipboard;
using static Fsync.Interop.Tests.ClipboardTestSupport;

namespace Fsync.Interop.Tests;

[Collection("clipboard")]
public sealed class ClipboardReadTests
{
    [Fact]
    public void PlainTextIsReadAndNotSkipped()
    {
        WriteText("hello clipboard ✓");
        var s = Read();
        Assert.Equal(SkipReason.None, s.Skip);
        Assert.Equal("hello clipboard ✓", s.Text);
        Assert.DoesNotContain("hello", s.Describe()); // logs carry names and sizes only
    }

    [Theory]
    [InlineData(ClipboardFormats.ExcludeFromMonitor, 1u, SkipReason.ExcludeFromMonitor)]
    [InlineData(ClipboardFormats.ViewerIgnore, 1u, SkipReason.ViewerIgnore)]
    [InlineData(ClipboardFormats.CanIncludeInHistory, 0u, SkipReason.NoClipboardHistory)]
    [InlineData(ClipboardFormats.CanUploadToCloud, 0u, SkipReason.NoCloudUpload)]
    [InlineData(ClipboardFormats.CanIncludeInHistory, 1u, SkipReason.None)]
    [InlineData(ClipboardFormats.FsyncOrigin, 1u, SkipReason.OwnEcho)]
    public void SensitiveFlagsSkipTheChangeWithoutReadingContent(string flag, uint value, SkipReason expected)
    {
        WriteText("secret-password", (flag, value));
        var s = Read();
        Assert.Equal(expected, s.Skip);
        if (expected != SkipReason.None) Assert.Null(s.Text);
    }

    [Fact]
    public async Task RichTextImageAndHtmlAreRead()
    {
        var (png, _) = await ImageCodecTests.MakePngAsync();
        var rtf = Encoding.ASCII.GetBytes(@"{\rtf1\ansi {\b bold}}");
        Win32Clipboard.Write([
            (ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes("bold\0")),
            (ClipboardFormats.Register(ClipboardFormats.Html), CfHtmlCodec.Encode("<b>bold</b>")),
            (ClipboardFormats.Register(ClipboardFormats.Rtf), rtf),
            (ClipboardFormats.Register(ClipboardFormats.Png), png),
        ]);
        var s = Read();
        Assert.Equal("bold", s.Text);
        Assert.Equal("<b>bold</b>", CfHtmlCodec.Decode(s.Html!));
        Assert.Equal(rtf, s.Rtf![..rtf.Length]); // HGLOBALs may be rounded up
        Assert.Equal(png, s.Png![..png.Length]);
        Assert.Null(s.Dib);
    }

    [Fact]
    public async Task DibIsReadWhenNoPngIsOffered()
    {
        var (png, pixels) = await ImageCodecTests.MakePngAsync();
        Win32Clipboard.Write([(ClipboardFormats.CF_DIB, await Imaging.ImageCodec.PngToDibAsync(png))]);
        var s = Read();
        Assert.Null(s.Png);
        Assert.NotNull(s.Dib);
        Assert.Equal(pixels, await ImageCodecTests.PixelsAsync(await Imaging.ImageCodec.DibToPngAsync(s.Dib!)));
    }

    [Fact]
    public void FileListIsRead()
    {
        var dir = TempDir();
        try
        {
            var a = Path.Combine(dir, "a.txt");
            File.WriteAllText(a, "x");
            Win32Clipboard.Write([(ClipboardFormats.CF_HDROP, DropFiles.Build([a, dir]))]);
            Assert.Equal([a, dir], Read().Files);
        }
        finally { DeleteDir(dir); }
    }

    [Fact]
    public void LargeContentIsNotReadPastTheLimit()
    {
        WriteText(new string('x', 10_000));
        var s = ClipboardReader.Read(maxBytes: 1000, []);
        Assert.Equal(SkipReason.TooLarge, s.Skip);
        Assert.Null(s.Text);
    }

    [Fact]
    public void EmptyClipboardHasNoContent()
    {
        Win32Clipboard.Write([]);
        Assert.Equal(SkipReason.NoContent, Read().Skip);
    }
}
