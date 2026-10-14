using System.Text;
using Fsync.Interop.Clipboard;
using static Fsync.Interop.Tests.ClipboardTestSupport;

namespace Fsync.Interop.Tests;

/// <summary>Entries fsync owns: eager / delayed formats, echo marker, listener, exit policy.</summary>
[Collection("clipboard")]
public sealed class ClipboardOwnTests : IDisposable
{
    private readonly StaThread _sta = new("test-clipboard");

    private static ClipEntry Text(string s) => ClipEntry.Eager(ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes(s + "\0"));

    private ClipDataObject Own(params ClipEntry[] entries) => _sta.Invoke(() =>
    {
        var o = new ClipDataObject(entries);
        OleClipboard.Set(o);
        return o;
    });

    [Fact]
    public void DelayedFormatRendersOnlyWhenRead()
    {
        var renders = 0;
        var html = ClipEntry.Lazy(ClipboardFormats.Html, 100, () => { renders++; return CfHtmlCodec.Encode("<b>late</b>"); });
        var obj = Own(Text("now"), html);

        var snap = Read();
        Assert.Equal(SkipReason.OwnEcho, snap.Skip); // FsyncOrigin: never re-published
        Assert.Equal(0, renders);
        Assert.Equal(100, obj.PendingLazyBytes);

        byte[]? got = null;
        Assert.True(Win32Clipboard.TryWithOpen(() => got = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Html))));
        Assert.Equal("<b>late</b>", CfHtmlCodec.Decode(got!));
        Assert.Equal(1, renders);
        Assert.Equal(0, obj.PendingLazyBytes);
        Assert.True(Win32Clipboard.TryWithOpen(() => got = Win32Clipboard.Read(ClipboardFormats.CF_UNICODETEXT)));
        Assert.Equal("now", Encoding.Unicode.GetString(got!).TrimEnd('\0'));
    }

    [Fact]
    public void FailedRenderIsReportedAndTheReaderGetsNothing()
    {
        Exception? failure = null;
        var obj = Own(ClipEntry.Lazy(ClipboardFormats.Rtf, 10, () => throw new IOException("peer offline")));
        obj.RenderFailed += (_, e) => failure = e;
        byte[]? got = [1];
        Assert.True(Win32Clipboard.TryWithOpen(() => got = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Rtf))));
        Assert.Null(got);
        Assert.IsType<IOException>(failure);
    }

    [Fact]
    public void ListenerSeesOtherWritesAndOurOwn()
    {
        var seen = 0;
        var listener = _sta.Invoke(() =>
        {
            var l = new ClipboardListener();
            l.Changed += _ => Interlocked.Increment(ref seen);
            return l;
        });
        try
        {
            WriteText("one");
            Assert.True(WaitFor(() => Volatile.Read(ref seen) >= 1));
            Own(Text("two"));
            Assert.True(WaitFor(() => Volatile.Read(ref seen) >= 2));
        }
        finally { _sta.Invoke(listener.Dispose); }
    }

    [Fact]
    public void OwnerProcessIsKnownForOleEntries()
    {
        Own(Text("x"));
        Assert.Equal(System.Diagnostics.Process.GetCurrentProcess().ProcessName, ClipboardOwner.ProcessName());
    }

    [Fact]
    public void FlushKeepsRenderedFormatsAfterRelease()
    {
        var obj = Own(Text("kept"), ClipEntry.Lazy(ClipboardFormats.Rtf, 5, () => Encoding.ASCII.GetBytes("{rtf}")));
        _sta.Invoke(OleClipboard.Flush);
        Assert.False(_sta.Invoke(() => OleClipboard.IsCurrent(obj)));
        byte[]? text = null, rtf = null;
        Assert.True(Win32Clipboard.TryWithOpen(() =>
        {
            text = Win32Clipboard.Read(ClipboardFormats.CF_UNICODETEXT);
            rtf = Win32Clipboard.Read(ClipboardFormats.Register(ClipboardFormats.Rtf));
        }));
        Assert.Equal("kept", Encoding.Unicode.GetString(text!).TrimEnd('\0'));
        Assert.Equal("{rtf}", Encoding.ASCII.GetString(rtf!).TrimEnd('\0'));
        Assert.Equal(SkipReason.OwnEcho, Read().Skip); // the marker survives the flush
    }

    [Fact]
    public void ClearEmptiesOnlyOurEntry()
    {
        var obj = Own(Text("mine"));
        _sta.Invoke(() => OleClipboard.ClearIfCurrent(obj));
        Assert.Equal(SkipReason.NoContent, Read().Skip);

        var old = Own(Text("old"));
        WriteText("someone else's");
        _sta.Invoke(() => OleClipboard.ClearIfCurrent(old));
        Assert.Equal("someone else's", Read().Text);
    }

    public void Dispose() => _sta.Dispose();
}
