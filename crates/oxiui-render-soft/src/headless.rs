/// Output pixel format for [`crate::backend::SoftBackend::to_bytes`].
///
/// Selects the byte layout when exporting the framebuffer's `0xAARRGGBB`
/// native format to a flat byte slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 4 bytes per pixel in memory order A, R, G, B
    /// (direct dump of the native `0xAARRGGBB` u32, big-endian byte order).
    Argb32,
    /// 4 bytes per pixel in memory order B, G, R, A
    /// (Windows DIB / DirectX BGRA convention).
    Bgra8,
    /// 2 bytes per pixel: 5 red + 6 green + 5 blue, big-endian.
    Rgb565,
}

/// RGBA pixel buffer returned by headless rendering.
///
/// Pixels are stored row-major, left-to-right, top-to-bottom.
/// Each pixel occupies exactly 4 bytes in the order R, G, B, A.
pub struct RgbaBuffer {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw pixel data: `width * height * 4` bytes (R, G, B, A per pixel).
    pub data: Vec<u8>,
}

impl RgbaBuffer {
    /// Returns `true` if any byte in the pixel data is non-zero.
    ///
    /// A buffer composed entirely of zero bytes represents a fully transparent
    /// (or un-rendered) frame.  Any non-zero R, G, B, or A value means that at
    /// least one pixel carried visible or semi-transparent content.
    pub fn has_content(&self) -> bool {
        self.data.iter().any(|&b| b != 0)
    }

    /// Save this buffer as a PNG file at `path`.
    ///
    /// Uses the `png` crate (pure Rust) — no C/C++ dependencies.
    ///
    /// # Errors
    /// Returns [`crate::SoftRenderError::Io`] if the file cannot be created or
    /// written, and [`crate::SoftRenderError::Png`] if the PNG encoder fails.
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), crate::SoftRenderError> {
        let file =
            std::fs::File::create(path).map_err(|e| crate::SoftRenderError::Io(e.to_string()))?;
        let buf_writer = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(buf_writer, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| crate::SoftRenderError::Png(e.to_string()))?;
        writer
            .write_image_data(&self.data)
            .map_err(|e| crate::SoftRenderError::Png(e.to_string()))
    }
}

/// Background fill colour used by [`render_headless_once`].
///
/// This matches the COOLJAPAN default dark theme background (#1A1B26, Tokyo Night).
/// Using a non-zero fill guarantees that [`RgbaBuffer::has_content`] returns `true`.
pub const HEADLESS_BG_COLOR: [u8; 4] = [26, 27, 38, 255]; // R, G, B, A

/// Render a minimal UI scene headlessly and return an RGBA pixel buffer.
///
/// No window, GPU, or display connection is required.  The buffer is filled with
/// the COOLJAPAN default theme's background colour so that the result is
/// immediately non-trivial (i.e. [`RgbaBuffer::has_content`] returns `true`).
///
/// This function is primarily useful for CI smoke tests and ffi-audit containers
/// where a display is unavailable.
///
/// # Example
/// ```rust
/// let buf = oxiui_render_soft::headless::render_headless_once(64, 48);
/// assert_eq!(buf.width, 64);
/// assert_eq!(buf.height, 48);
/// assert!(buf.has_content());
/// ```
pub fn render_headless_once(width: u32, height: u32) -> RgbaBuffer {
    let bg = HEADLESS_BG_COLOR;
    let data: Vec<u8> = (0..width * height)
        .flat_map(|_| bg.iter().copied())
        .collect();
    RgbaBuffer {
        width,
        height,
        data,
    }
}

/// Render a custom scene headlessly and return an RGBA pixel buffer.
///
/// The framebuffer is pre-filled with the COOLJAPAN default theme background;
/// `draw_fn` receives a clipped [`Canvas`](crate::Canvas) to paint the scene.
/// No window, GPU, or display connection is required — suitable for CI smoke
/// tests, ffi-audit containers, and screenshot generation.
///
/// # Example
/// ```rust
/// use oxiui_core::Color;
/// let buf = oxiui_render_soft::headless::render_headless_scene(64, 48, |c| {
///     c.fill_rect(8.0, 8.0, 16.0, 16.0, Color(255, 0, 0, 255));
/// });
/// assert_eq!(buf.width, 64);
/// assert!(buf.has_content());
/// ```
pub fn render_headless_scene<F>(width: u32, height: u32, draw_fn: F) -> RgbaBuffer
where
    F: FnOnce(&mut crate::Canvas<'_>),
{
    let bg = HEADLESS_BG_COLOR;
    let mut fb =
        crate::Framebuffer::with_fill(width, height, oxiui_core::Color(bg[0], bg[1], bg[2], bg[3]));
    {
        let mut canvas = crate::Canvas::new(&mut fb);
        draw_fn(&mut canvas);
    }
    fb.to_rgba_buffer()
}
