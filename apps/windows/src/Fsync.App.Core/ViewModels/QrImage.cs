using QRCoder;

namespace Fsync.App.ViewModels;

/// <summary>PNG bytes of a QR code for the pairing payload (view turns them into a bitmap).</summary>
public static class QrImage
{
    public static byte[] Png(string payload, int pixelsPerModule = 8)
    {
        using var gen = new QRCodeGenerator();
        using var data = gen.CreateQrCode(payload, QRCodeGenerator.ECCLevel.M);
        return new PngByteQRCode(data).GetGraphic(pixelsPerModule);
    }
}
