import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

/// QR rendering: a pure function plus a view with hard pixels.
public enum QrCode {
    /// A crisp, square QR for `text`, or nil when it can't be encoded. `scale` is pixels per module.
    public static func image(for text: String, scale: CGFloat = 8) -> CGImage? {
        guard let data = text.data(using: .utf8), !data.isEmpty else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = data
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scaled = output.samplingNearest().transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        return CIContext().createCGImage(scaled, from: scaled.extent)
    }
}

struct QrCodeImageView: View {
    let text: String
    var side: CGFloat = 200

    var body: some View {
        if let cg = QrCode.image(for: text) {
            Image(decorative: cg, scale: 1)
                .interpolation(.none)
                .resizable()
                .frame(width: side, height: side)
                .padding(10)
                .background(Color.white)  // QR needs a light quiet zone, also in dark mode
                .accessibilityLabel("Pairing QR code")
        } else {
            Text("Can't draw the QR code. Use the code instead.").foregroundStyle(.secondary)
        }
    }
}
