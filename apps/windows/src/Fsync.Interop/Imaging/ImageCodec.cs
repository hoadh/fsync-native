using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Fsync.Interop.Imaging;

/// <summary>
/// CF_DIB / CF_DIBV5 ↔ PNG with the WinRT imaging codecs (WIC). The wire format for images is
/// `image/png`; Windows apps that do not read "PNG" get a CF_DIB.
/// </summary>
public static class ImageCodec
{
    private const int FileHeaderSize = 14;
    private const uint BI_BITFIELDS = 3, BI_ALPHABITFIELDS = 6;
    /// <summary>Largest image we decode (sides and pixel count): a peer must not make us allocate gigabytes.</summary>
    public const uint MaxSide = 16384, MaxPixels = 64_000_000;

    /// <summary>PNG bytes for a packed DIB (BITMAPINFOHEADER or V4/V5 header + pixels).</summary>
    public static async Task<byte[]> DibToPngAsync(byte[] dib)
    {
        var bmp = DibToBmpFile(dib);
        using var input = await ToStreamAsync(bmp);
        var decoder = await BitmapDecoder.CreateAsync(BitmapDecoder.BmpDecoderId, input);
        CheckPixels(decoder);
        using var bitmap = await decoder.GetSoftwareBitmapAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied);
        return await EncodeAsync(BitmapEncoder.PngEncoderId, bitmap);
    }

    /// <summary>A packed 32-bpp DIB (BITMAPINFOHEADER, bottom-up) for PNG bytes.</summary>
    public static async Task<byte[]> PngToDibAsync(byte[] png)
    {
        using var input = await ToStreamAsync(png);
        var decoder = await BitmapDecoder.CreateAsync(input);
        CheckPixels(decoder);
        using var bitmap = await decoder.GetSoftwareBitmapAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Ignore);
        var bmp = await EncodeAsync(BitmapEncoder.BmpEncoderId, bitmap);
        return bmp.Length > FileHeaderSize && bmp[0] == 'B' && bmp[1] == 'M'
            ? bmp[FileHeaderSize..]
            : throw new InvalidDataException("BMP encoder returned no file header");
    }

    /// <summary>Pixel size of an image (PNG or any WIC format); for previews and tests.</summary>
    public static async Task<(uint Width, uint Height)> SizeAsync(byte[] image)
    {
        using var input = await ToStreamAsync(image);
        var decoder = await BitmapDecoder.CreateAsync(input);
        return (decoder.PixelWidth, decoder.PixelHeight);
    }

    private static void CheckPixels(BitmapDecoder d)
    {
        if (d.PixelWidth == 0 || d.PixelHeight == 0 || d.PixelWidth > MaxSide || d.PixelHeight > MaxSide || (ulong)d.PixelWidth * d.PixelHeight > MaxPixels)
            throw new InvalidDataException($"image too large ({d.PixelWidth}x{d.PixelHeight})");
    }

    /// <summary>Prepends the BITMAPFILEHEADER a packed DIB lacks, so the BMP decoder accepts it.</summary>
    internal static byte[] DibToBmpFile(byte[] dib)
    {
        if (dib.Length < 40) throw new InvalidDataException("DIB too short");
        var headerSize = BitConverter.ToUInt32(dib, 0);
        if (headerSize is < 12 or > 1024 || headerSize >= dib.Length) throw new InvalidDataException("bad DIB header");
        uint offset = headerSize;
        if (headerSize >= 40)
        {
            var bitCount = BitConverter.ToUInt16(dib, 14);
            var compression = BitConverter.ToUInt32(dib, 16);
            var clrUsed = BitConverter.ToUInt32(dib, 32);
            if (headerSize == 40 && compression == BI_BITFIELDS) offset += 12; // three DWORD masks follow
            if (headerSize == 40 && compression == BI_ALPHABITFIELDS) offset += 16; // four masks
            var colors = clrUsed != 0 ? clrUsed : bitCount <= 8 ? 1u << bitCount : 0;
            if (colors > 256 && bitCount <= 8 || colors > dib.Length / 4) throw new InvalidDataException("bad DIB colour table");
            offset += colors * 4;
            if (offset >= dib.Length) throw new InvalidDataException("DIB has no pixels");
        }
        var file = new byte[FileHeaderSize + dib.Length];
        file[0] = (byte)'B';
        file[1] = (byte)'M';
        BitConverter.GetBytes((uint)file.Length).CopyTo(file, 2);
        BitConverter.GetBytes(FileHeaderSize + offset).CopyTo(file, 10);
        dib.CopyTo(file, FileHeaderSize);
        return file;
    }

    private static async Task<byte[]> EncodeAsync(Guid encoderId, SoftwareBitmap bitmap)
    {
        using var output = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(encoderId, output);
        encoder.SetSoftwareBitmap(bitmap);
        await encoder.FlushAsync();
        var bytes = new byte[output.Size];
        output.Seek(0);
        using var reader = new DataReader(output.GetInputStreamAt(0));
        await reader.LoadAsync((uint)output.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    private static async Task<InMemoryRandomAccessStream> ToStreamAsync(byte[] bytes)
    {
        var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
        {
            writer.WriteBytes(bytes);
            await writer.StoreAsync();
            await writer.FlushAsync();
            writer.DetachStream();
        }
        stream.Seek(0);
        return stream;
    }
}
