using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Fsync.App.Tests;

/// <summary>Real PNGs made with WIC, and their decoded pixels for comparisons.</summary>
internal static class TestImages
{
    public static async Task<byte[]> PngAsync(uint w, uint h)
    {
        var pixels = new byte[w * h * 4];
        for (var i = 0; i < w * h; i++)
        {
            pixels[i * 4] = (byte)(i * 7);
            pixels[i * 4 + 1] = (byte)(i / 5);
            pixels[i * 4 + 2] = (byte)(255 - i);
            pixels[i * 4 + 3] = 255;
        }
        using var stream = new InMemoryRandomAccessStream();
        var enc = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        enc.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Straight, w, h, 96, 96, pixels);
        await enc.FlushAsync();
        var bytes = new byte[stream.Size];
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        await reader.LoadAsync((uint)stream.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    public static async Task<byte[]> PixelsAsync(byte[] image)
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
}
