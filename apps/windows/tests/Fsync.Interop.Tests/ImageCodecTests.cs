using Fsync.Interop.Imaging;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Fsync.Interop.Tests;

public sealed class ImageCodecTests
{
    /// <summary>A real 37x23 PNG with a distinct colour per pixel (odd sizes catch stride bugs).</summary>
    internal static async Task<(byte[] Png, byte[] Pixels)> MakePngAsync(uint w = 37, uint h = 23)
    {
        var pixels = new byte[w * h * 4];
        for (var i = 0; i < w * h; i++)
        {
            pixels[i * 4] = (byte)(i * 7);       // B
            pixels[i * 4 + 1] = (byte)(i / 3);   // G
            pixels[i * 4 + 2] = (byte)(255 - i); // R
            pixels[i * 4 + 3] = 255;             // A (opaque: CF_DIB has no alpha)
        }
        using var stream = new InMemoryRandomAccessStream();
        var enc = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        enc.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Straight, w, h, 96, 96, pixels);
        await enc.FlushAsync();
        return (await ReadAllAsync(stream), pixels);
    }

    internal static async Task<byte[]> PixelsAsync(byte[] image)
    {
        using var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
        {
            writer.WriteBytes(image);
            await writer.StoreAsync();
            writer.DetachStream();
        }
        stream.Seek(0);
        var dec = await BitmapDecoder.CreateAsync(stream);
        var data = await dec.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Straight,
            new BitmapTransform(), ExifOrientationMode.IgnoreExifOrientation, ColorManagementMode.DoNotColorManage);
        return data.DetachPixelData();
    }

    private static async Task<byte[]> ReadAllAsync(IRandomAccessStream s)
    {
        var bytes = new byte[s.Size];
        using var reader = new DataReader(s.GetInputStreamAt(0));
        await reader.LoadAsync((uint)s.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    [Fact]
    public async Task PngToDibAndBackKeepsEveryPixel()
    {
        var (png, pixels) = await MakePngAsync();
        var dib = await ImageCodec.PngToDibAsync(png);
        Assert.Equal(40u, BitConverter.ToUInt32(dib, 0)); // BITMAPINFOHEADER
        Assert.Equal(37, BitConverter.ToInt32(dib, 4));
        var back = await ImageCodec.DibToPngAsync(dib);
        Assert.Equal((37u, 23u), await ImageCodec.SizeAsync(back));
        Assert.Equal(pixels, await PixelsAsync(back));
    }

    [Fact]
    public void BmpFileHeaderOffsetAccountsForMasksAndPalette()
    {
        var dib = new byte[40 + 12 + 16];
        BitConverter.GetBytes(40u).CopyTo(dib, 0);
        BitConverter.GetBytes((ushort)32).CopyTo(dib, 14);
        BitConverter.GetBytes(3u).CopyTo(dib, 16); // BI_BITFIELDS
        Assert.Equal(14u + 40 + 12, BitConverter.ToUInt32(ImageCodec.DibToBmpFile(dib), 10));

        var pal = new byte[40 + 4 * 256 + 64];
        BitConverter.GetBytes(40u).CopyTo(pal, 0);
        BitConverter.GetBytes((ushort)8).CopyTo(pal, 14);
        Assert.Equal(14u + 40 + 1024, BitConverter.ToUInt32(ImageCodec.DibToBmpFile(pal), 10));
        Assert.Throws<InvalidDataException>(() => ImageCodec.DibToBmpFile([1, 2, 3]));
    }

    /// <summary>Distinct opaque pixels, B,G,R,A per pixel, row 0 first (top).</summary>
    private static byte[] DistinctPixels(int w, int h)
    {
        var px = new byte[w * h * 4];
        for (var i = 0; i < w * h; i++)
        {
            px[i * 4] = (byte)(i * 7);
            px[i * 4 + 1] = (byte)(i * 3 + 1);
            px[i * 4 + 2] = (byte)(255 - i);
            px[i * 4 + 3] = 255;
        }
        return px;
    }

    /// <summary>A packed bottom-up DIB: 40-byte BITMAPINFOHEADER, optional mask bytes, then the pixels of <paramref name="px"/> (BGRA, top-down) as 24 or 32 bpp.</summary>
    private static byte[] BuildDib(byte[] px, int w, int h, int bpp, uint compression, uint[]? masks = null, int headerSize = 40)
    {
        var bytesPP = bpp / 8;
        var stride = (w * bytesPP + 3) & ~3;
        var maskBytes = masks is null ? 0 : masks.Length * 4;
        var dib = new byte[headerSize + maskBytes + stride * h];
        BitConverter.GetBytes((uint)headerSize).CopyTo(dib, 0);
        BitConverter.GetBytes(w).CopyTo(dib, 4);
        BitConverter.GetBytes(h).CopyTo(dib, 8); // positive: bottom-up
        BitConverter.GetBytes((ushort)1).CopyTo(dib, 12);
        BitConverter.GetBytes((ushort)bpp).CopyTo(dib, 14);
        BitConverter.GetBytes(compression).CopyTo(dib, 16);
        BitConverter.GetBytes((uint)(stride * h)).CopyTo(dib, 20);
        // Masks: after a 40-byte header they are the only place; V4/V5 carry them in the header AND (as the
        // synthesized CF_DIBV5 of Windows does) repeated right after it.
        for (var m = 0; masks is not null && m < masks.Length; m++)
        {
            BitConverter.GetBytes(masks[m]).CopyTo(dib, 40 + m * 4);
            if (headerSize > 40) BitConverter.GetBytes(masks[m]).CopyTo(dib, headerSize + m * 4);
        }
        for (var y = 0; y < h; y++)
            for (var x = 0; x < w; x++)
                for (var c = 0; c < bytesPP; c++)
                    dib[headerSize + maskBytes + (h - 1 - y) * stride + x * bytesPP + c] = px[(y * w + x) * 4 + c];
        return dib;
    }

    private static async Task AssertDibPixelsAsync(byte[] dib, byte[] expected, int w, int h)
    {
        var png = await ImageCodec.DibToPngAsync(dib);
        Assert.Equal(((uint)w, (uint)h), await ImageCodec.SizeAsync(png));
        Assert.Equal(expected, await PixelsAsync(png));
    }

    // 40x40 like the real clipboard: Clipboard.SetImage writes a BI_BITFIELDS 32-bpp DIB with a 40-byte header.
    [Fact]
    public async Task BitfieldsDibWithStandardMasksKeepsEveryPixel()
    {
        var px = DistinctPixels(40, 40);
        var dib = BuildDib(px, 40, 40, 32, 3, [0x00FF0000u, 0x0000FF00u, 0x000000FFu]);
        await AssertDibPixelsAsync(dib, px, 40, 40);
    }

    // The app reads CF_DIBV5 first; Windows synthesizes it from SetImage's BITFIELDS DIB as a 124-byte header
    // followed by 12 more mask bytes before the pixels (found by cross-OS test X3: the PNG was shifted 3 pixels).
    [Fact]
    public async Task SynthesizedDibV5WithTrailingMasksKeepsEveryPixel()
    {
        var px = DistinctPixels(40, 40);
        var dib = BuildDib(px, 40, 40, 32, 3, [0x00FF0000u, 0x0000FF00u, 0x000000FFu], headerSize: 124);
        await AssertDibPixelsAsync(dib, px, 40, 40);
    }

    [Fact]
    public async Task RgbDib32KeepsEveryPixel()
    {
        var px = DistinctPixels(37, 23);
        await AssertDibPixelsAsync(BuildDib(px, 37, 23, 32, 0), px, 37, 23);
    }

    [Fact]
    public async Task RgbDib24KeepsEveryPixel()
    {
        var px = DistinctPixels(37, 23);
        await AssertDibPixelsAsync(BuildDib(px, 37, 23, 24, 0), px, 37, 23);
    }

    [Fact]
    public async Task BitfieldsDibWithNonStandardMasksIsRejected()
    {
        var px = DistinctPixels(8, 8);
        var dib = BuildDib(px, 8, 8, 32, 3, [0x000000FFu, 0x0000FF00u, 0x00FF0000u]); // RGB order
        await Assert.ThrowsAsync<InvalidDataException>(() => ImageCodec.DibToPngAsync(dib));
    }
}
