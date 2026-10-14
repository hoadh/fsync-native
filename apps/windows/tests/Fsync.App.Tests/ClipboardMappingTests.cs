using System.Text;
using Fsync.App.Clipboard;
using Fsync.Core;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Imaging;

namespace Fsync.App.Tests;

[CollectionDefinition("clipboard", DisableParallelization = true)]
public sealed class ClipboardCollection : ICollectionFixture<ClipboardLock>;

/// <summary>
/// Out (clipboard → LocalClip) and in (ClipItem → clipboard entry) mapping, round-tripped
/// through the real system clipboard: what one PC publishes is what the other PC pastes.
/// </summary>
[Collection("clipboard")]
public sealed class ClipboardMappingTests : IDisposable
{
    private readonly StaThread _sta = new("test-mapping");

    private static ClipboardSnapshot Snapshot() => ClipboardReader.Read(long.MaxValue, []);

    private static ClipItem Item(LocalClip clip) => new(
        Guid.NewGuid().ToString(), new string('b', 64), "Mac mini", ClipKind.Rich, 1, clip.preview,
        [.. clip.reps.Select(r => new RepInfo(r.mime, (ulong)r.data.Length))], [], 0, false);

    /// <summary>Writes a received item like the service does; returns how often each rep was fetched.</summary>
    private int[] WriteReceived(LocalClip clip)
    {
        var fetched = new int[clip.reps.Length];
        var (entries, files) = IncomingClip.Build(Item(clip), i => { fetched[i]++; return clip.reps[i].data; }, (_, _, _) => []);
        _sta.Invoke(() => OleClipboard.Set(new ClipDataObject(entries, files)));
        return fetched;
    }

    [Fact]
    public async Task RichTextKeepsHtmlRtfAndTextInThatOrder()
    {
        var rtf = Encoding.ASCII.GetBytes(@"{\rtf1\ansi {\b Xin ch\u224?o}}");
        Win32Clipboard.Write([
            (ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes("Xin chào\0")),
            (ClipboardFormats.Register(ClipboardFormats.Html), CfHtmlCodec.Encode("<b>Xin chào</b>")),
            (ClipboardFormats.Register(ClipboardFormats.Rtf), rtf),
        ]);
        var clip = (await RepresentationMapper.ToLocalClipAsync(Snapshot()))!;
        Assert.Equal(["text/html", "text/rtf", "text/plain"], clip.reps.Select(r => r.mime));
        Assert.Equal("<b>Xin chào</b>", Encoding.UTF8.GetString(clip.reps[0].data));
        Assert.Equal(rtf, clip.reps[1].data);
        Assert.Equal("Xin chào", clip.preview);

        WriteReceived(clip);
        var back = Snapshot();
        Assert.Equal(SkipReason.OwnEcho, back.Skip); // our write is never published again
        Assert.Equal(clip.reps.Select(r => r.mime), (await RepresentationMapper.ToLocalClipAsync(ReadIgnoringEcho()))!.reps.Select(r => r.mime));
    }

    /// <summary>The content of our own entry, read as another app would (skip rules aside).</summary>
    private static ClipboardSnapshot ReadIgnoringEcho()
    {
        byte[]? text = null, html = null, rtf = null, png = null, dib = null;
        Assert.True(Win32Clipboard.TryWithOpen(() =>
        {
            text = Win32Clipboard.Read(ClipboardFormats.CF_UNICODETEXT);
            html = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Html));
            rtf = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Rtf));
            png = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Png));
            dib = Win32Clipboard.Read(ClipboardFormats.CF_DIB);
        }));
        return new ClipboardSnapshot
        {
            Formats = [],
            Text = text is null ? null : Encoding.Unicode.GetString(text).TrimEnd('\0'),
            Html = html, Rtf = rtf, Png = png, Dib = dib,
        };
    }

    [Fact]
    public async Task PlainTextRoundTrips()
    {
        Win32Clipboard.Write([(ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes("plain ✓ text\0"))]);
        var clip = (await RepresentationMapper.ToLocalClipAsync(Snapshot()))!;
        Assert.Equal("text/plain", Assert.Single(clip.reps).mime);
        WriteReceived(clip);
        Assert.Equal("plain ✓ text", ReadIgnoringEcho().Text);
    }

    [Fact]
    public async Task DibImageIsPublishedAsPngAndPastedAsBoth()
    {
        var png = await TestImages.PngAsync(300, 300); // > 256 KiB as a DIB, small as PNG
        Win32Clipboard.Write([(ClipboardFormats.CF_DIB, await ImageCodec.PngToDibAsync(png))]);
        var clip = (await RepresentationMapper.ToLocalClipAsync(Snapshot()))!;
        var rep = Assert.Single(clip.reps);
        Assert.Equal("image/png", rep.mime);
        Assert.Equal("Image", clip.preview);
        Assert.Equal(await TestImages.PixelsAsync(png), await TestImages.PixelsAsync(rep.data));

        WriteReceived(clip);
        var back = ReadIgnoringEcho();
        Assert.Equal(rep.data, back.Png![..rep.data.Length]);
        Assert.Equal(await TestImages.PixelsAsync(png), await TestImages.PixelsAsync(await ImageCodec.DibToPngAsync(back.Dib!)));
    }

    [Fact]
    public void LargeRepsAreFetchedOnlyWhenATargetReadsThem()
    {
        var big = new string('x', 300_000);
        var clip = new LocalClip([new Rep("text/plain", Encoding.UTF8.GetBytes(big))], [], null, false);
        var fetched = WriteReceived(clip);
        Assert.Equal(0, fetched[0]);
        Assert.Equal(big, ReadIgnoringEcho().Text);
        Assert.Equal(1, fetched[0]);

        var small = new LocalClip([new Rep("text/plain", "tiny"u8.ToArray())], [], null, false);
        Assert.Equal(1, WriteReceived(small)[0]); // inline-sized: fetched before writing
    }

    [Fact]
    public async Task FilesArePublishedAsPathsWithANamePreview()
    {
        var dir = Path.Combine(Path.GetTempPath(), "fsync-map-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(dir);
        try
        {
            var a = Path.Combine(dir, "report.pdf");
            File.WriteAllText(a, "x");
            Win32Clipboard.Write([(ClipboardFormats.CF_HDROP, DropFiles.Build([a, dir, Path.Combine(dir, "gone.txt")]))]);
            var clip = (await RepresentationMapper.ToLocalClipAsync(Snapshot()))!;
            Assert.Equal([a, dir], clip.files); // vanished paths are dropped
            Assert.Empty(clip.reps);
            Assert.Equal("report.pdf and 1 more", clip.preview);
        }
        finally { Directory.Delete(dir, true); }
    }

    [Fact]
    public async Task SkippedOrEmptyClipboardPublishesNothing()
    {
        Win32Clipboard.Write([
            (ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes("pw\0")),
            (ClipboardFormats.Register(ClipboardFormats.ExcludeFromMonitor), BitConverter.GetBytes(1u)),
        ]);
        Assert.Null(await RepresentationMapper.ToLocalClipAsync(Snapshot()));
        Win32Clipboard.Write([]);
        Assert.Null(await RepresentationMapper.ToLocalClipAsync(Snapshot()));
    }

    [Fact]
    public void UnknownMimeTypesAreNotWritten()
    {
        var clip = new LocalClip([new Rep("application/x-unknown", [1, 2]), new Rep("text/plain", "ok"u8.ToArray())], [], null, false);
        var (entries, _) = IncomingClip.Build(Item(clip), i => clip.reps[i].data, (_, _, _) => []);
        Assert.Equal([ClipboardFormats.CF_UNICODETEXT], entries.Select(e => e.Format));
    }

    public void Dispose()
    {
        _sta.Invoke(() => OleClipboard.Set(new ClipDataObject([])));
        _sta.Dispose();
    }
}
