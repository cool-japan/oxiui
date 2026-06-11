//! [`SoftBackend`]: a [`oxiui_core::paint::RenderBackend`] implementation that replays a
//! [`oxiui_core::paint::DrawList`] onto a CPU [`Framebuffer`] through a single [`Canvas`].
//!
//! Because all commands share one [`Canvas`] for the entire
//! [`oxiui_core::paint::RenderBackend::execute`] call,
//! `PushClip`/`PopClip` commands are correctly applied to every subsequent
//! draw command — there is no clip-leak between commands.

use oxiui_core::geometry::Size;
use oxiui_core::paint::{DrawCommand, DrawList, ImageFilter, RenderBackend};
use oxiui_core::{Color, UiError};

use crate::clip::{ClipRect, ClipStack};
use crate::draw::{Canvas, DashPattern, SrcImage};
use crate::framebuffer::Framebuffer;
use crate::shadow::GaussianCache;
use crate::{AaMode, ShadowQuality, SoftRenderQuality};

// ---------------------------------------------------------------------------
// SoftBackend
// ---------------------------------------------------------------------------

/// CPU framebuffer backend implementing [`oxiui_core::paint::RenderBackend`].
///
/// Replays a [`oxiui_core::paint::DrawList`] onto a [`Framebuffer`] via a [`Canvas`]
/// (clip-correct). All commands share one clip stack for the duration of
/// the [`oxiui_core::paint::RenderBackend::execute`] call.
///
/// When the `text` feature is enabled, a cached `oxiui_text::TextPipeline`
/// is held on the backend so that `DrawText` commands are fully shaped and
/// rasterised rather than silently ignored.
pub struct SoftBackend {
    fb: Framebuffer,
    shadow_cache: GaussianCache,
    /// Quality preset controlling AA mode and shadow rendering strategy.
    quality: SoftRenderQuality,
    /// Optional cached text pipeline.  `None` when the `text` feature is
    /// disabled or when the embedded font fails to parse (graceful degradation).
    #[cfg(feature = "text")]
    text_pipeline: Option<oxiui_text::TextPipeline>,
}

impl SoftBackend {
    /// Create a backend with a transparent framebuffer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            fb: Framebuffer::new(width, height),
            shadow_cache: GaussianCache::default(),
            quality: SoftRenderQuality::balanced(),
            #[cfg(feature = "text")]
            text_pipeline: Self::init_text_pipeline(),
        }
    }

    /// Create a backend pre-filled with a solid background colour.
    pub fn with_background(width: u32, height: u32, bg: Color) -> Self {
        Self {
            fb: Framebuffer::with_fill(width, height, bg),
            shadow_cache: GaussianCache::default(),
            quality: SoftRenderQuality::balanced(),
            #[cfg(feature = "text")]
            text_pipeline: Self::init_text_pipeline(),
        }
    }

    /// Initialise the embedded-font text pipeline.
    ///
    /// Returns `None` (graceful degradation) if the font fails to parse,
    /// rather than panicking.
    #[cfg(feature = "text")]
    fn init_text_pipeline() -> Option<oxiui_text::TextPipeline> {
        // Embedded test font — shared with the oxitext test suite.
        // Path is relative to this source file (src/backend.rs):
        //   src/ → oxiui-render-soft/ → crates/ → oxiui/ → noffi/ → oxitext/…
        const FONT_BYTES: &[u8] =
            include_bytes!("../../../../oxitext/tests/fixtures/test-font.ttf");
        oxiui_text::TextPipeline::from_bytes(FONT_BYTES).ok()
    }

    /// Set the quality preset, influencing AA mode and shadow strategy.
    pub fn set_quality(&mut self, quality: SoftRenderQuality) {
        self.quality = quality;
    }

    /// Return the current quality preset.
    pub fn quality(&self) -> &SoftRenderQuality {
        &self.quality
    }

    /// Return `true` if the current quality preset enables anti-aliasing.
    fn aa_enabled(&self) -> bool {
        !matches!(self.quality.aa_mode, AaMode::None)
    }

    /// Return `true` if shadow rendering is enabled at any quality level.
    fn shadow_enabled(&self) -> bool {
        !matches!(self.quality.shadow_quality, ShadowQuality::Off)
    }

    /// Borrow the underlying framebuffer.
    pub fn frame(&self) -> &Framebuffer {
        &self.fb
    }

    /// Consume the backend, returning the underlying framebuffer.
    pub fn into_framebuffer(self) -> Framebuffer {
        self.fb
    }

    /// Fill the entire framebuffer with `color`.
    pub fn clear(&mut self, color: Color) {
        self.fb.clear(color);
    }

    /// Apply a [`oxiui_theme::ShadowSpec`] to the framebuffer at the given rect.
    ///
    /// Bridges the theme-level shadow description into the existing Gaussian-blur
    /// shadow infrastructure in [`crate::shadow`].  The rect is inflated by
    /// `spec.spread` before blurring, matching CSS box-shadow behaviour.
    ///
    /// `inset` shadows are currently rendered the same as drop shadows
    /// (the inset flag is noted but not yet composited differently — a follow-up
    /// can apply source-atop compositing for true inset support).
    ///
    /// # Parameters
    /// * `rect` — bounding rectangle `(x, y, width, height)` of the widget.
    /// * `spec` — the shadow specification from the theme.
    #[cfg(feature = "theme")]
    pub fn apply_shadow_spec(
        &mut self,
        rect: (f32, f32, f32, f32),
        spec: &oxiui_theme::ShadowSpec,
    ) {
        use crate::shadow::box_shadow;

        // Inflate rect by spread radius.
        let (rx, ry, rw, rh) = rect;
        let spread = spec.spread;
        let inflated = (
            rx - spread,
            ry - spread,
            rw + spread * 2.0,
            rh + spread * 2.0,
        );

        box_shadow(
            &mut self.fb,
            inflated,
            spec.offset_x,
            spec.offset_y,
            spec.blur,
            spec.color,
            &mut self.shadow_cache,
        );
    }

    /// Return the framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.fb.width()
    }

    /// Return the framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.fb.height()
    }

    /// Export the framebuffer as a flat byte vector in the requested pixel format.
    ///
    /// The native internal format is `0xAARRGGBB` (one `u32` per pixel).
    /// This method reinterprets and reorders channels for the target format.
    ///
    /// | Format | Bytes/pixel | Channel order |
    /// |--------|-------------|---------------|
    /// | `Argb32` | 4 | A, R, G, B |
    /// | `Bgra8`  | 4 | B, G, R, A |
    /// | `Rgb565` | 2 | 5R·6G·5B, big-endian |
    pub fn to_bytes(&self, format: crate::headless::PixelFormat) -> Vec<u8> {
        use crate::framebuffer::unpack;
        use crate::headless::PixelFormat;
        match format {
            PixelFormat::Argb32 => {
                // 4 bytes per pixel: A, R, G, B (unpack 0xAARRGGBB)
                self.fb
                    .pixels()
                    .iter()
                    .flat_map(|&p| {
                        let (r, g, b, a) = unpack(p);
                        [a, r, g, b]
                    })
                    .collect()
            }
            PixelFormat::Bgra8 => {
                // 4 bytes per pixel: B, G, R, A
                self.fb
                    .pixels()
                    .iter()
                    .flat_map(|&p| {
                        let (r, g, b, a) = unpack(p);
                        [b, g, r, a]
                    })
                    .collect()
            }
            PixelFormat::Rgb565 => {
                // 2 bytes per pixel: 5 red + 6 green + 5 blue, big-endian
                self.fb
                    .pixels()
                    .iter()
                    .flat_map(|&p| {
                        let (r, g, b, _a) = unpack(p);
                        let r5 = (r as u16) >> 3;
                        let g6 = (g as u16) >> 2;
                        let b5 = (b as u16) >> 3;
                        let rgb565: u16 = (r5 << 11) | (g6 << 5) | b5;
                        [(rgb565 >> 8) as u8, (rgb565 & 0xFF) as u8]
                    })
                    .collect()
            }
        }
    }
}

impl RenderBackend for SoftBackend {
    fn surface_size(&self) -> Size {
        Size::new(self.fb.width() as f32, self.fb.height() as f32)
    }

    fn supports_blur(&self) -> bool {
        true
    }
    fn supports_gradients(&self) -> bool {
        true
    }
    fn supports_paths(&self) -> bool {
        true
    }
    fn supports_images(&self) -> bool {
        true
    }

    #[cfg(feature = "text")]
    fn supports_text(&self) -> bool {
        self.text_pipeline.is_some()
    }

    #[cfg(not(feature = "text"))]
    fn supports_text(&self) -> bool {
        false
    }

    fn execute(&mut self, list: &DrawList) -> Result<(), UiError> {
        let aa = self.aa_enabled();
        let shadow = self.shadow_enabled();
        let fb_w = self.fb.width();
        let fb_h = self.fb.height();

        // We maintain a shadow clip stack so that when we break out of a Canvas
        // scope to blit text directly onto self.fb, we know the effective clip
        // rect.  The stack holds individual (unmerged) push rects so Canvas
        // clip state can be replayed when we re-enter after a DrawText.
        //
        // `pending_clips` mirrors the Canvas clip stack: each push appends one
        // entry and each pop removes the last entry.
        let mut pending_clips: Vec<(f32, f32, f32, f32)> = Vec::new();

        // Shadow clip stack for direct-fb text blitting: intersected region.
        let mut shadow_clip = ClipStack::new(fb_w, fb_h);

        // Iterator over the command list.
        let mut iter = list.iter().peekable();

        while iter.peek().is_some() {
            // ── Phase A: drain non-DrawText commands through a Canvas ──────
            {
                let mut canvas = Canvas::new(&mut self.fb);
                canvas.set_aa(aa);

                // Re-apply accumulated clips to the fresh canvas.
                for &(x, y, w, h) in &pending_clips {
                    canvas.push_clip(x, y, w, h);
                }

                // Process commands until we hit a DrawText (or exhaust the list).
                loop {
                    let peek = iter.peek();
                    let is_text = matches!(peek, Some(DrawCommand::DrawText { .. }));
                    if peek.is_none() || is_text {
                        break;
                    }
                    let cmd = iter.next().expect("peeked Some above");

                    // Track clip stack for shadow_clip mirror.
                    match cmd {
                        DrawCommand::PushClip { rect } => {
                            let r = (rect.left(), rect.top(), rect.width(), rect.height());
                            pending_clips.push(r);
                            shadow_clip.push(ClipRect::from_rect(
                                rect.left().floor() as i64,
                                rect.top().floor() as i64,
                                rect.width().ceil() as i64,
                                rect.height().ceil() as i64,
                            ));
                        }
                        DrawCommand::PopClip => {
                            pending_clips.pop();
                            shadow_clip.pop();
                        }
                        _ => {}
                    }

                    dispatch_command(&mut canvas, &mut self.shadow_cache, cmd, shadow);
                }
                // Canvas is dropped here, releasing the &mut self.fb borrow.
            }

            // ── Phase B: process one DrawText with direct fb access ────────
            #[cfg(feature = "text")]
            if matches!(iter.peek(), Some(DrawCommand::DrawText { .. })) {
                if let Some(DrawCommand::DrawText {
                    text, font, color, ..
                }) = iter.next()
                {
                    draw_text_to_fb(
                        &mut self.fb,
                        &mut self.text_pipeline,
                        text,
                        font,
                        *color,
                        shadow_clip.current(),
                    );
                }
                continue;
            }

            // Without the text feature, consume DrawText silently.
            #[cfg(not(feature = "text"))]
            if matches!(iter.peek(), Some(DrawCommand::DrawText { .. })) {
                iter.next();
                continue;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Text rendering helper (feature-gated)
// ---------------------------------------------------------------------------

/// Shape and rasterise `text` into `fb` at the glyph positions reported by the
/// pipeline, clipped to `clip`.
///
/// On pipeline error (e.g. missing glyph data) the error is silently ignored so
/// that a single broken glyph does not abort the entire frame — consistent with
/// `supports_text() == false` graceful degradation.
#[cfg(feature = "text")]
fn draw_text_to_fb(
    fb: &mut Framebuffer,
    pipeline: &mut Option<oxiui_text::TextPipeline>,
    text: &str,
    font: &oxiui_core::FontSpec,
    color: Color,
    clip: ClipRect,
) {
    let pipeline = match pipeline {
        Some(p) => p,
        None => return, // no font loaded; graceful no-op
    };

    if text.is_empty() {
        return;
    }

    let style = oxiui_text::TextStyle::new(font.size);
    let result = match pipeline.render(text, &style) {
        Ok(r) => r,
        Err(_) => return, // shaping error → silent skip
    };

    for (pg, bm) in result.glyphs.iter().zip(result.bitmaps.iter()) {
        if bm.is_empty() {
            continue;
        }
        let blit_x = pg.pos.0.round() as i32;
        let blit_y = pg.pos.1.round() as i32;
        blit_glyph_clipped(
            fb, blit_x, blit_y, bm.width, bm.height, &bm.pixels, color, clip,
        );
    }
}

// ---------------------------------------------------------------------------
// Glyph blitting primitives
// ---------------------------------------------------------------------------

/// Blit a greyscale alpha-coverage bitmap into `fb` at `(origin_x, origin_y)`,
/// tinting each pixel with `color`, clipped to `clip`.
///
/// `pixels` is row-major greyscale: one byte per pixel, 0 = transparent,
/// 255 = fully opaque.  Out-of-bounds pixels and pixels outside `clip` are
/// silently skipped.  The framebuffer stays in straight-alpha `0xAARRGGBB`.
#[cfg(feature = "text")]
#[allow(clippy::too_many_arguments)]
fn blit_glyph_clipped(
    fb: &mut Framebuffer,
    origin_x: i32,
    origin_y: i32,
    bm_width: u32,
    bm_height: u32,
    pixels: &[u8],
    color: Color,
    clip: ClipRect,
) {
    let fb_w = fb.width() as i32;
    let fb_h = fb.height() as i32;
    for row in 0..bm_height {
        for col in 0..bm_width {
            let coverage = pixels[(row * bm_width + col) as usize];
            if coverage == 0 {
                continue;
            }
            let px = origin_x + col as i32;
            let py = origin_y + row as i32;
            // Bounds check.
            if px < 0 || py < 0 || px >= fb_w || py >= fb_h {
                continue;
            }
            // Clip rect check.
            if !clip.contains(px as i64, py as i64) {
                continue;
            }
            // Scale the text colour's alpha by the glyph coverage, then blend.
            let effective_alpha = ((color.3 as u32) * (coverage as u32) / 255) as u8;
            let tinted = Color(color.0, color.1, color.2, effective_alpha);
            fb.blend_coverage(px as u32, py as u32, &tinted, 1.0);
        }
    }
}

/// Blit a greyscale alpha-coverage bitmap into the canvas at `(origin_x, origin_y)`,
/// tinting each pixel with `color`.
///
/// `pixels` is a row-major greyscale slice: one byte per pixel, 0 = transparent,
/// 255 = fully opaque.  Out-of-bounds pixels are silently skipped; the clip
/// stack is NOT applied (blending goes directly to the framebuffer).  Callers
/// that need clip-aware blitting should call through the [`Canvas`] draw API.
///
/// This is the primitive used for glyph blitting: feed in a rasterised glyph
/// bitmap and the text colour; each coverage byte is multiplied with `color.a`
/// to produce the final alpha before source-over compositing.
pub fn blit_glyph_bitmap(
    fb: &mut crate::framebuffer::Framebuffer,
    origin_x: i32,
    origin_y: i32,
    bm_width: u32,
    bm_height: u32,
    pixels: &[u8],
    color: oxiui_core::Color,
) {
    let fb_w = fb.width() as i32;
    let fb_h = fb.height() as i32;
    for row in 0..bm_height {
        for col in 0..bm_width {
            let alpha = pixels[(row * bm_width + col) as usize];
            if alpha == 0 {
                continue;
            }
            let px = origin_x + col as i32;
            let py = origin_y + row as i32;
            if px < 0 || py < 0 || px >= fb_w || py >= fb_h {
                continue;
            }
            // Scale the text colour's alpha by the glyph coverage.
            let effective_alpha = ((color.3 as u32) * (alpha as u32) / 255) as u8;
            let tinted = oxiui_core::Color(color.0, color.1, color.2, effective_alpha);
            fb.blend_coverage(px as u32, py as u32, &tinted, 1.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

/// Dispatch a single [`DrawCommand`] to the appropriate [`Canvas`] method.
///
/// Anti-aliasing is carried by `canvas.aa` (set once per `execute` call).
/// `shadow` enables Gaussian blur for box-shadow commands.
fn dispatch_command(
    canvas: &mut Canvas<'_>,
    cache: &mut GaussianCache,
    cmd: &DrawCommand,
    shadow: bool,
) {
    match cmd {
        // ── Clipping ──────────────────────────────────────────────────────
        DrawCommand::PushClip { rect } => {
            canvas.push_clip(rect.left(), rect.top(), rect.width(), rect.height());
        }
        DrawCommand::PopClip => {
            canvas.pop_clip();
        }

        // ── Rectangles ────────────────────────────────────────────────────
        DrawCommand::FillRect { rect, color } => {
            canvas.fill_rect(rect.left(), rect.top(), rect.width(), rect.height(), *color);
        }
        DrawCommand::StrokeRect {
            rect,
            thickness,
            color,
        } => {
            canvas.stroke_rect(
                rect.left(),
                rect.top(),
                rect.width(),
                rect.height(),
                *thickness,
                *color,
            );
        }
        DrawCommand::FillRoundedRect {
            rect,
            radius,
            color,
        } => {
            canvas.fill_rounded_rect(
                rect.left(),
                rect.top(),
                rect.width(),
                rect.height(),
                *radius,
                *color,
            );
        }
        DrawCommand::FillRoundedRectPerCorner { rect, radii, color } => {
            canvas.fill_rounded_rect_per_corner(
                rect.left(),
                rect.top(),
                rect.width(),
                rect.height(),
                *radii,
                *color,
            );
        }

        // ── Circles / Ellipses ────────────────────────────────────────────
        DrawCommand::FillCircle {
            center,
            radius,
            color,
        } => {
            canvas.fill_circle(center.x, center.y, *radius, *color);
        }
        DrawCommand::FillEllipse {
            center,
            rx,
            ry,
            color,
        } => {
            canvas.fill_ellipse(center.x, center.y, *rx, *ry, *color);
        }

        // ── Lines ─────────────────────────────────────────────────────────
        DrawCommand::Line { from, to, color } => {
            canvas.draw_line(from.x, from.y, to.x, to.y, *color);
        }
        DrawCommand::LineAa { from, to, color } => {
            canvas.draw_line_wu(from.x, from.y, to.x, to.y, *color);
        }
        DrawCommand::LineThick {
            from,
            to,
            width,
            color,
        } => {
            canvas.draw_line_thick(from.x, from.y, to.x, to.y, *width, *color);
        }
        DrawCommand::LineDashed {
            from,
            to,
            dash_len,
            gap_len,
            color,
        } => {
            canvas.draw_line_dashed(
                from.x,
                from.y,
                to.x,
                to.y,
                *color,
                DashPattern::new(*dash_len, *gap_len),
            );
        }

        // ── Paths ─────────────────────────────────────────────────────────
        DrawCommand::FillPath { path, color } => {
            canvas.fill_path(path, *color);
        }
        DrawCommand::StrokePath { path, style, color } => {
            canvas.stroke_path(path, style, *color);
        }

        // ── Gradients ─────────────────────────────────────────────────────
        DrawCommand::LinearGradient {
            rect,
            start,
            end,
            stops,
        } => {
            canvas.fill_linear_gradient_cmd(*rect, *start, *end, stops);
        }
        DrawCommand::RadialGradient {
            rect,
            center,
            radius,
            stops,
        } => {
            canvas.fill_radial_gradient_cmd(*rect, *center, *radius, stops);
        }

        // ── Images ────────────────────────────────────────────────────────
        DrawCommand::Image {
            image,
            dest,
            filter,
        } => {
            let src = SrcImage::new(&image.rgba, image.width, image.height);
            let dst_w = dest.width().round() as u32;
            let dst_h = dest.height().round() as u32;
            match filter {
                ImageFilter::Nearest => {
                    canvas.blit_rgba(src, dest.left(), dest.top(), dst_w, dst_h);
                }
                ImageFilter::Bilinear => {
                    canvas.blit_bilinear(src, dest.left(), dest.top(), dst_w, dst_h);
                }
            }
        }
        DrawCommand::NineSlice {
            image,
            dest,
            insets,
        } => {
            let src = SrcImage::new(&image.rgba, image.width, image.height);
            let dst_w = dest.width().round() as u32;
            let dst_h = dest.height().round() as u32;
            canvas.blit_nine_slice(src, dest.left(), dest.top(), dst_w, dst_h, *insets);
        }

        // ── Shadows ───────────────────────────────────────────────────────
        DrawCommand::BoxShadow {
            rect,
            offset,
            blur_radius,
            color,
        } if shadow => {
            canvas.box_shadow_cmd(*rect, *offset, *blur_radius, *color, cache);
        }
        DrawCommand::BoxShadow { .. } => {
            // ShadowQuality::Off → skip entirely (no artifact).
        }

        // ── Text ─────────────────────────────────────────────────────────
        // DrawText is handled in execute() before dispatch_command is called.
        DrawCommand::DrawText { .. } => {}

        // non_exhaustive fallback
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;
    #[cfg(feature = "text")]
    use oxiui_core::paint::DrawList;
    use oxiui_core::Color;
    #[cfg(feature = "text")]
    use oxiui_core::{geometry::Rect, FontSpec};

    /// Test 1: blit an 8×8 all-255 alpha bitmap, verify framebuffer pixel at
    /// (0,0) matches the text colour.
    #[test]
    fn glyph_blit_all_opaque_sets_color() {
        let mut fb = Framebuffer::new(8, 8);
        let pixels = vec![255u8; 8 * 8];
        let color = Color(255, 0, 128, 255); // opaque magenta-ish
        blit_glyph_bitmap(&mut fb, 0, 0, 8, 8, &pixels, color);
        let (r, g, b, a) = fb.get_rgba(0, 0).expect("pixel at (0,0)");
        assert_eq!(r, 255, "red channel mismatch");
        assert_eq!(g, 0, "green channel mismatch");
        assert!(b > 100 && b < 160, "blue channel off: {b}");
        assert_eq!(a, 255, "alpha channel mismatch");
    }

    /// Blit out-of-bounds glyph pixels must not panic.
    #[test]
    fn glyph_blit_oob_is_noop() {
        let mut fb = Framebuffer::new(4, 4);
        let pixels = vec![200u8; 4 * 4];
        // Negative origin: glyph is entirely off-screen.
        blit_glyph_bitmap(&mut fb, -10, -10, 4, 4, &pixels, Color(255, 0, 0, 255));
        // Nothing should have changed.
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 0)));
    }

    /// Blit with zero-alpha pixels must leave framebuffer unchanged.
    #[test]
    fn glyph_blit_zero_alpha_is_noop() {
        let mut fb = Framebuffer::with_fill(4, 4, Color(10, 20, 30, 255));
        let pixels = vec![0u8; 4 * 4]; // all transparent
        blit_glyph_bitmap(&mut fb, 0, 0, 4, 4, &pixels, Color(255, 0, 0, 255));
        assert_eq!(fb.get_rgba(0, 0), Some((10, 20, 30, 255)));
    }

    /// Partial alpha glyph pixel produces blended colour.
    #[test]
    fn glyph_blit_partial_alpha_blends() {
        let mut fb = Framebuffer::with_fill(4, 4, Color(0, 0, 0, 255));
        let mut pixels = vec![0u8; 4 * 4];
        pixels[0] = 128; // one semi-transparent pixel at top-left
        blit_glyph_bitmap(&mut fb, 0, 0, 4, 4, &pixels, Color(255, 0, 0, 255));
        let (r, _g, _b, _a) = fb.get_rgba(0, 0).expect("pixel");
        // Red over black with ~50% alpha → red should be around 127.
        assert!(r > 50, "expected partial blend, got r={r}");
    }

    // ── to_bytes tests ──────────────────────────────────────────────────────

    /// Test 8: to_bytes(Argb32).len() == 4 * width * height
    #[test]
    fn to_bytes_argb32_correct_length() {
        use crate::headless::PixelFormat;
        let backend = SoftBackend::new(10, 8);
        let bytes = backend.to_bytes(PixelFormat::Argb32);
        assert_eq!(bytes.len(), 4 * 10 * 8);
    }

    /// Test 9: Bgra8 byte order: R and B channels swapped vs Argb32.
    #[test]
    fn to_bytes_bgra8_rb_swap() {
        use crate::headless::PixelFormat;
        // Fill with a known colour: R=10, G=20, B=30, A=255.
        let mut backend = SoftBackend::new(1, 1);
        backend.clear(Color(10, 20, 30, 255));
        let argb = backend.to_bytes(PixelFormat::Argb32);
        // Argb32 layout: [A, R, G, B]
        assert_eq!(argb[0], 255, "Argb32[0]=A");
        assert_eq!(argb[1], 10, "Argb32[1]=R");
        assert_eq!(argb[2], 20, "Argb32[2]=G");
        assert_eq!(argb[3], 30, "Argb32[3]=B");

        let bgra = backend.to_bytes(PixelFormat::Bgra8);
        // Bgra8 layout: [B, G, R, A]
        assert_eq!(bgra[0], 30, "Bgra8[0]=B");
        assert_eq!(bgra[1], 20, "Bgra8[1]=G");
        assert_eq!(bgra[2], 10, "Bgra8[2]=R");
        assert_eq!(bgra[3], 255, "Bgra8[3]=A");
    }

    /// Test 10: Rgb565 output has exactly 2 bytes per pixel.
    #[test]
    fn to_bytes_rgb565_correct_length() {
        use crate::headless::PixelFormat;
        let backend = SoftBackend::new(7, 5);
        let bytes = backend.to_bytes(PixelFormat::Rgb565);
        assert_eq!(bytes.len(), 2 * 7 * 5);
    }

    /// Rgb565 preserves the high bits of each channel.
    #[test]
    fn to_bytes_rgb565_round_trip_high_bits() {
        use crate::headless::PixelFormat;
        // Pure red: R=255, G=0, B=0 → R5=31, G6=0, B5=0 → 0b11111_000000_00000 = 0xF800
        let mut backend = SoftBackend::new(1, 1);
        backend.clear(Color(255, 0, 0, 255));
        let bytes = backend.to_bytes(PixelFormat::Rgb565);
        let word = (bytes[0] as u16) << 8 | (bytes[1] as u16);
        let r5 = word >> 11;
        assert_eq!(r5, 31, "pure red should give max R5");
        let g6 = (word >> 5) & 0x3F;
        assert_eq!(g6, 0, "pure red should give zero G6");
    }

    /// Quality setter round-trips.
    #[test]
    fn backend_quality_round_trip() {
        use crate::{AaMode, ShadowQuality, SoftRenderQuality};
        let mut backend = SoftBackend::new(10, 10);
        let q = SoftRenderQuality::low();
        backend.set_quality(q.clone());
        assert_eq!(backend.quality().aa_mode, AaMode::None);
        assert_eq!(backend.quality().shadow_quality, ShadowQuality::Off);
    }

    // ── Text feature tests ──────────────────────────────────────────────────

    /// Test: supports_text() returns true when the text feature is enabled.
    #[cfg(feature = "text")]
    #[test]
    fn supports_text_true_with_feature() {
        let backend = SoftBackend::new(100, 100);
        // The embedded font must load; if not, the test is still valid
        // (supports_text returns true only when pipeline is Some).
        // We assert the pipeline loaded successfully for a proper test.
        assert!(backend.text_pipeline.is_some(), "embedded font must parse");
        assert!(
            backend.supports_text(),
            "supports_text must be true with font loaded"
        );
    }

    /// Test: supports_text() returns false without the text feature.
    #[cfg(not(feature = "text"))]
    #[test]
    fn supports_text_false_without_feature() {
        let backend = SoftBackend::new(100, 100);
        assert!(!backend.supports_text());
    }

    /// Test: DrawText with the text feature produces non-zero pixels in the
    /// expected region.
    #[cfg(feature = "text")]
    #[test]
    fn draw_text_produces_pixels() {
        let mut backend = SoftBackend::new(200, 50);
        // Start with a black background so text pixels are distinguishable.
        backend.clear(Color(0, 0, 0, 255));

        let mut dl = DrawList::new();
        dl.push_text(
            Rect::new(0.0, 0.0, 200.0, 50.0),
            "A",
            FontSpec::default(),
            Color(255, 255, 255, 255),
        );
        backend.execute(&dl).expect("execute must succeed");

        // At least one pixel in the framebuffer must be non-black after rendering "A".
        let has_nonblack = backend.frame().pixels().iter().any(|&px| px != 0xFF000000);
        assert!(
            has_nonblack,
            "DrawText 'A' must produce at least one non-black pixel"
        );
    }

    /// Test: DrawText respects an active clip rect.  When the clip excludes the
    /// text origin entirely, no pixels should be modified.
    #[cfg(feature = "text")]
    #[test]
    fn draw_text_clip_rect_excludes() {
        let mut backend = SoftBackend::new(200, 50);
        // Fill with a known background.
        backend.clear(Color(0, 0, 0, 255));

        let mut dl = DrawList::new();
        // Clip to the right half only; render text in the left half.
        // The clip must exclude the glyph completely.
        dl.push_clip(Rect::new(150.0, 0.0, 50.0, 50.0));
        dl.push_text(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            "A",
            FontSpec::default(),
            Color(255, 255, 255, 255),
        );
        dl.pop_clip();
        backend.execute(&dl).expect("execute must succeed");

        // All pixels outside the clip region (x < 150) must remain black.
        // We check pixels in the left 150 columns.
        let fb = backend.frame();
        for y in 0..50u32 {
            for x in 0..150u32 {
                let px = fb.get(x, y).unwrap_or(0);
                assert_eq!(
                    px, 0xFF000000,
                    "pixel ({x},{y}) must be unchanged (clipped), got 0x{px:08X}"
                );
            }
        }
    }

    /// Test: drawing an empty string must not panic and must not modify the fb.
    #[cfg(feature = "text")]
    #[test]
    fn draw_text_no_panic_empty_string() {
        let mut backend = SoftBackend::new(100, 50);
        backend.clear(Color(0, 0, 0, 255));

        let mut dl = DrawList::new();
        dl.push_text(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            "",
            FontSpec::default(),
            Color(255, 255, 255, 255),
        );
        backend.execute(&dl).expect("execute must succeed");

        // All pixels must remain unchanged.
        let has_nonblack = backend.frame().pixels().iter().any(|&px| px != 0xFF000000);
        assert!(!has_nonblack, "empty string must not modify any pixels");
    }

    /// Test: rendering "AB" produces non-empty pixels across a wider region
    /// than rendering "A" alone, verifying horizontal advance.
    #[cfg(feature = "text")]
    #[test]
    fn draw_text_horizontal_advance() {
        // Render "A" alone.
        let mut backend_a = SoftBackend::new(200, 50);
        backend_a.clear(Color(0, 0, 0, 255));
        let mut dl_a = DrawList::new();
        dl_a.push_text(
            Rect::new(0.0, 0.0, 200.0, 50.0),
            "A",
            FontSpec::default(),
            Color(255, 255, 255, 255),
        );
        backend_a.execute(&dl_a).expect("execute A");

        // Render "AB".
        let mut backend_ab = SoftBackend::new(200, 50);
        backend_ab.clear(Color(0, 0, 0, 255));
        let mut dl_ab = DrawList::new();
        dl_ab.push_text(
            Rect::new(0.0, 0.0, 200.0, 50.0),
            "AB",
            FontSpec::default(),
            Color(255, 255, 255, 255),
        );
        backend_ab.execute(&dl_ab).expect("execute AB");

        // "AB" must have at least as many non-black pixels as "A".
        let count_nonblack = |fb: &Framebuffer| -> usize {
            fb.pixels().iter().filter(|&&px| px != 0xFF000000).count()
        };
        let count_a = count_nonblack(backend_a.frame());
        let count_ab = count_nonblack(backend_ab.frame());
        assert!(
            count_ab >= count_a,
            "rendering 'AB' must cover at least as many pixels as 'A' (a={count_a}, ab={count_ab})"
        );
        // Both must be non-empty.
        assert!(count_a > 0, "'A' must produce some non-black pixels");
        assert!(count_ab > 0, "'AB' must produce some non-black pixels");
    }

    // ── Clip-aware blit_glyph_clipped tests ────────────────────────────────

    /// blit_glyph_clipped with a clip that excludes the glyph entirely →
    /// no pixels changed.
    #[cfg(feature = "text")]
    #[test]
    fn blit_glyph_clipped_excludes_fully() {
        let mut fb = Framebuffer::with_fill(10, 10, Color(0, 0, 0, 255));
        let pixels = vec![255u8; 4 * 4];
        // Glyph at (0,0), clip is entirely in the right half.
        let clip = ClipRect {
            x0: 5,
            y0: 0,
            x1: 10,
            y1: 10,
        };
        blit_glyph_clipped(
            &mut fb,
            0,
            0,
            4,
            4,
            &pixels,
            Color(255, 255, 255, 255),
            clip,
        );
        // Left half must be unchanged.
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
        assert_eq!(fb.get_rgba(4, 0), Some((0, 0, 0, 255)));
    }

    /// blit_glyph_clipped with a full-fb clip paints all glyph pixels.
    #[cfg(feature = "text")]
    #[test]
    fn blit_glyph_clipped_full_clip_paints() {
        let mut fb = Framebuffer::new(4, 4);
        let pixels = vec![255u8; 4 * 4];
        let clip = ClipRect::full(4, 4);
        blit_glyph_clipped(&mut fb, 0, 0, 4, 4, &pixels, Color(255, 0, 0, 255), clip);
        assert!(fb.get(0, 0).unwrap_or(0) != 0, "pixel should be non-zero");
    }
}
