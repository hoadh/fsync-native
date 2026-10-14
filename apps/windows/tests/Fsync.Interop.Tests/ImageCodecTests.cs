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
}
