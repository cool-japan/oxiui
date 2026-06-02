//! PNG icon decoding: decode raw PNG bytes → [`egui::IconData`].
//!
//! This module is only compiled when the `egui` feature is enabled (which
//! guarantees the `png` crate is also present as a transitive dependency).

#[cfg(feature = "egui")]
/// Decode raw PNG bytes into an [`egui::IconData`] value suitable for
/// passing to [`egui::ViewportBuilder::with_icon`].
///
/// Supports RGBA8, RGB8, Grayscale, and GrayscaleAlpha PNG colour modes.
/// Any unsupported colour type returns [`crate::UiError::Other`].
///
/// # Errors
///
/// - [`crate::UiError::Other`] on PNG decode failure or unsupported colour type.
pub(crate) fn decode_icon(bytes: &[u8]) -> Result<egui::IconData, crate::UiError> {
    use std::io::Cursor;

    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| crate::UiError::Other(format!("PNG decode error: {e}")))?;

    let buf_size = reader.output_buffer_size().ok_or_else(|| {
        crate::UiError::Other("PNG output buffer size unavailable (bit depth not 8?)".to_string())
    })?;
    let mut buf = vec![0u8; buf_size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| crate::UiError::Other(format!("PNG frame error: {e}")))?;

    let width = info.width;
    let height = info.height;

    // Convert to RGBA8 if not already.
    let rgba_bytes: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()]
            .chunks(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255u8])
            .collect(),
        png::ColorType::Grayscale => buf[..info.buffer_size()]
            .iter()
            .flat_map(|&g| [g, g, g, 255u8])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()]
            .chunks(2)
            .flat_map(|ga| [ga[0], ga[0], ga[0], ga[1]])
            .collect(),
        _ => {
            return Err(crate::UiError::Other(
                "unsupported PNG colour type for icon".to_string(),
            ));
        }
    };

    Ok(egui::IconData {
        rgba: rgba_bytes,
        width,
        height,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[cfg(feature = "egui")]
mod tests {
    use super::*;

    /// Write a minimal 4×4 RGBA PNG to a temp file and decode it.
    #[test]
    fn test_icon_decode_png() {
        use std::io::BufWriter;

        let tmp = std::env::temp_dir().join("oxiui_icon_test_4x4.png");
        let file = std::fs::File::create(&tmp).expect("create temp file");
        let mut encoder = png::Encoder::new(BufWriter::new(file), 4, 4);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write PNG header");
        // 4×4 RGBA pixels, all opaque red.
        let pixels = [255u8, 0, 0, 255].repeat(16);
        writer.write_image_data(&pixels).expect("write PNG data");
        drop(writer);

        let bytes = std::fs::read(&tmp).expect("read temp file");
        let _ = std::fs::remove_file(&tmp);

        let icon = decode_icon(&bytes).expect("decode_icon must succeed");
        assert_eq!(icon.width, 4);
        assert_eq!(icon.height, 4);
        assert_eq!(icon.rgba.len(), 64, "4×4 RGBA = 64 bytes");
    }

    /// Passing garbage bytes must return an error, not panic.
    #[test]
    fn test_icon_decode_invalid_bytes() {
        let result = decode_icon(b"not a png");
        assert!(result.is_err(), "invalid bytes must return Err");
    }
}
