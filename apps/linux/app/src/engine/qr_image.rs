//! QR code as a `data:image/png;base64,...` URL, so QML shows it with a plain `Image` (no
//! qt6-svg dependency).

use base64::Engine as _;
use image::{ImageBuffer, ImageFormat, Luma};
use qrcode::{Color, EcLevel, QrCode};
use std::io::Cursor;

const SCALE: u32 = 6;
const QUIET: u32 = 4;

pub fn png_data_url(payload: &str) -> Result<String, String> {
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M)
        .map_err(|e| format!("QR encode: {e}"))?;
    let modules = code.width() as u32;
    let side = (modules + 2 * QUIET) * SCALE;
    let colors = code.to_colors();
    let img = ImageBuffer::from_fn(side, side, |x, y| {
        let (mx, my) = (x / SCALE, y / SCALE);
        let dark = mx >= QUIET
            && my >= QUIET
            && mx < modules + QUIET
            && my < modules + QUIET
            && colors[((my - QUIET) * modules + (mx - QUIET)) as usize] == Color::Dark;
        Luma([if dark { 0u8 } else { 255u8 }])
    });
    let mut png = Cursor::new(Vec::new());
    img.write_to(&mut png, ImageFormat::Png)
        .map_err(|e| format!("QR png: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
    Ok(format!("data:image/png;base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_url_decodes_to_a_square_png() {
        let url = png_data_url("fsync://pair?n=abc&s=def").unwrap();
        let b64 = url.strip_prefix("data:image/png;base64,").expect("prefix");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let img = image::load_from_memory_with_format(&bytes, ImageFormat::Png).unwrap();
        assert_eq!(img.width(), img.height());
        assert!(img.width() > 100);
    }

    #[test]
    fn oversize_payload_is_an_error_not_a_panic() {
        assert!(png_data_url(&"x".repeat(5000)).is_err());
    }
}
