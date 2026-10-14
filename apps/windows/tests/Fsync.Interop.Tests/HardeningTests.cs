using System.Text;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Imaging;
using Fsync.Interop.Shell;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;
using static Fsync.Interop.Tests.ClipboardTestSupport;

namespace Fsync.Interop.Tests;

/// <summary>Review round 1: privacy flags vs echo marker, denylist, names, image bounds, STA thread.</summary>
[Collection("clipboard")]
public sealed class HardeningTests
{
    [Fact]
    public void ReceivedClipsOptOutOfHistoryAndCloudButStayOurEcho()
    {
        using var sta = new StaThread("test-flags");
        sta.Invoke(() => OleClipboard.Set(new ClipDataObject(
            [ClipEntry.Eager(ClipboardFormats.CF_UNICODETEXT, Encoding.Unicode.GetBytes("peer text\0")), .. ClipEntry.PrivacyFlags()])));
        var snap = Read();
        Assert.Equal(SkipReason.OwnEcho, snap.Skip); // FsyncOrigin wins over the private flags
        uint? history = null, cloud = null;
        var hasExclude = false;
        Assert.True(Win32Clipboard.TryWithOpen(() =>
        {
            history = Win32Clipboard.ReadDword(ClipboardFormats.Register(ClipboardFormats.CanIncludeInHistory));
            cloud = Win32Clipboard.ReadDword(ClipboardFormats.Register(ClipboardFormats.CanUploadToCloud));
            hasExclude = Win32Clipboard.Formats().Contains(ClipboardFormats.Register(ClipboardFormats.ExcludeFromMonitor));
        }));
        Assert.Equal(0u, history);
        Assert.Equal(0u, cloud);
        Assert.True(hasExclude);
        sta.Invoke(() => OleClipboard.Set(new ClipDataObject([])));
    }

    [Fact]
    public void BuiltInPasswordManagersAreAlwaysDenied()
    {
        var effective = SensitiveFormatFilter.EffectiveDenylist(["MyVault", " keepass ", ""]);
        Assert.Contains("1Password", effective);
        Assert.Contains("MyVault", effective);
        Assert.Single(effective, d => d.Equals("KeePass", StringComparison.OrdinalIgnoreCase));
        Assert.True(SensitiveFormatFilter.IsDenied("Bitwarden", SensitiveFormatFilter.EffectiveDenylist(null)));
        Assert.True(SensitiveFormatFilter.IsDenied("Bitwarden", SensitiveFormatFilter.EffectiveDenylist([])));
    }

    [Theory]
    [InlineData("CON")] [InlineData("nul.txt")] [InlineData("dir/COM1.log")] [InlineData("LPT9")] [InlineData("aux .txt")] [InlineData("COM¹")]
    public void ReservedDeviceNamesAreRejected(string rel) => Assert.Null(VirtualFile.Normalize(rel));

    [Theory]
    [InlineData("console.txt")] [InlineData("COM10")] [InlineData("nul_file")] [InlineData("my con/file.txt")]
    public void NamesThatOnlyLookReservedAreKept(string rel) => Assert.NotNull(VirtualFile.Normalize(rel));

    [Fact]
    public async Task OversizedImagesAreRefused()
    {
        var w = ImageCodec.MaxSide + 1;
        using var stream = new InMemoryRandomAccessStream();
        var enc = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        enc.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Straight, w, 1, 96, 96, new byte[w * 4]);
        await enc.FlushAsync();
        var png = new byte[stream.Size];
        using (var reader = new DataReader(stream.GetInputStreamAt(0)))
        {
            await reader.LoadAsync((uint)stream.Size);
            reader.ReadBytes(png);
        }
        await Assert.ThrowsAsync<InvalidDataException>(() => ImageCodec.PngToDibAsync(png));
    }

    [Fact]
    public void BrokenColourTablesAreRefused()
    {
        var dib = new byte[64];
        BitConverter.GetBytes(40u).CopyTo(dib, 0);
        BitConverter.GetBytes((ushort)8).CopyTo(dib, 14);
        BitConverter.GetBytes(0xFFFFFFFFu).CopyTo(dib, 32); // biClrUsed: would overflow the offset
        Assert.Throws<InvalidDataException>(() => ImageCodec.DibToBmpFile(dib));
    }

    [Fact]
    public async Task StaThreadRunsEveryQueuedItemFromManyThreads()
    {
        using var sta = new StaThread("test-wakeups");
        var count = 0;
        await Task.WhenAll(Enumerable.Range(0, 2000).Select(_ => Task.Run(() => sta.InvokeAsync(() => Interlocked.Increment(ref count)))));
        Assert.Equal(2000, count);
        Assert.Equal(5, sta.Invoke(() => sta.Invoke(() => 5))); // re-entrant call runs inline
    }

    [Fact]
    public async Task DisposeFailsQueuedWorkInsteadOfHanging()
    {
        var sta = new StaThread("test-dispose");
        using var gate = new ManualResetEventSlim();
        var blocker = sta.InvokeAsync(() => gate.Wait(2000));
        var queued = sta.InvokeAsync(() => 1);
        sta.Dispose(TimeSpan.FromMilliseconds(100));
        await Assert.ThrowsAsync<ObjectDisposedException>(() => queued.WaitAsync(TimeSpan.FromSeconds(5)));
        await Assert.ThrowsAsync<ObjectDisposedException>(() => sta.InvokeAsync(() => 2));
        gate.Set();
        Assert.True(await blocker.WaitAsync(TimeSpan.FromSeconds(5)));
    }
}
