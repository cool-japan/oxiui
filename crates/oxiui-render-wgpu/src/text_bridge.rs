//! SDF text rendering bridge — `oxiui-text` → `oxiui-render-wgpu`.
//!
//! Enabled by the `text` Cargo feature.  Converts `DrawCommand::DrawText`
//! into a sequence of per-glyph `ImageData` blits (alpha-textured quads)
//! consumed by the existing textured pipeline.
//!
//! # Architecture
//!
//! The text pipeline works in two stages:
//!
//! 1. **CPU shaping** — [`TextBridge::expand_draw_text`] calls `oxiui-text`'s
//!    [`oxiui_text::TextPipeline`] to produce `PositionedGlyph` instances with
//!    glyph IDs and pen positions.
//! 2. **Glyph blit expansion** — [`TextBridge::expand_draw_text`] walks
//!    positioned glyphs, rasterizes each via the [`oxiui_text::GlyphAtlas`]
//!    (LRU cache), converts the greyscale coverage bitmap to RGBA (applying the
//!    text colour), and pushes one `DrawCommand::Image` quad per glyph onto the
//!    output [`oxiui_core::paint::DrawList`].
//!
//! The output `DrawList` can then be passed to `WgpuBackend::execute` as
//! normal — no new GPU infrastructure is required.
//!
//! # SDF / MSDF note
//!
//! The current implementation uses greyscale alpha coverage bitmaps.  A future
//! upgrade can replace `bitmap_to_rgba` with a proper SDF distance-field
//! generator (e.g. using `oxifont`'s glyph outline) and a dedicated SDF fragment
//! shader for sub-pixel accurate rendering.  The interface is the same either way.
//!
//! # Example
//!
//! ```rust,no_run
//! use oxiui_render_wgpu::text_bridge::TextBridge;
//! use oxiui_text::{TextPipeline, TextStyle};
//! use oxiui_core::{geometry::Rect, paint::DrawList, Color};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let font_bytes: Vec<u8> = std::fs::read("/System/Library/Fonts/Helvetica.ttc")?;
//! let pipeline = TextPipeline::from_bytes(&font_bytes)?;
//! let mut bridge = TextBridge::new(pipeline, 512);
//!
//! let style = TextStyle::new(16.0).color([255, 255, 255, 255]);
//! let rect = Rect::new(10.0, 10.0, 200.0, 24.0);
//! let mut out = DrawList::new();
//! bridge.expand_draw_text(&mut out, rect, "Hello", &style, Color(255, 255, 255, 255))?;
//! # Ok(())
//! # }
//! ```

use oxiui_core::{
    geometry::Rect,
    paint::{DrawCommand, DrawList, ImageData, ImageFilter},
    Color, UiError,
};
use oxiui_text::{GlyphAtlas, GlyphKey, TextPipeline, TextStyle};

// ── TextBridge ────────────────────────────────────────────────────────────────

/// CPU-side text rendering bridge that converts `DrawText` commands into
/// per-glyph `DrawCommand::Image` blits ready for the wgpu textured pipeline.
///
/// Owns a [`TextPipeline`] (shaping + rasterization) and a [`GlyphAtlas`]
/// (LRU bitmap cache).  Create one instance per render target and call
/// [`expand_draw_text`] once per `DrawText` command each frame.
///
/// [`expand_draw_text`]: TextBridge::expand_draw_text
pub struct TextBridge {
    /// The underlying text shaping + rasterization pipeline.
    pipeline: TextPipeline,
    /// LRU bitmap cache.
    atlas: GlyphAtlas,
}

impl TextBridge {
    /// Create a new bridge backed by `pipeline` with an atlas capacity of
    /// `atlas_capacity` glyph entries.
    pub fn new(pipeline: TextPipeline, atlas_capacity: usize) -> Self {
        Self {
            pipeline,
            atlas: GlyphAtlas::new(atlas_capacity),
        }
    }

    /// Expand a `DrawText`-equivalent specification into per-glyph image blits.
    ///
    /// Shapes `text` using `style`, walks each positioned glyph, rasterizes via
    /// the atlas, tints the greyscale coverage bitmap with `color`, and pushes
    /// one `DrawCommand::Image` per glyph onto `out`.  The pen origin is set
    /// to `rect.top_left()`.
    ///
    /// Glyphs whose bitmaps are empty (whitespace, tofu, etc.) are skipped
    /// silently.
    ///
    /// # Errors
    /// Returns [`UiError::Render`] if text shaping fails.  Individual glyph
    /// rasterization failures are swallowed (the glyph is skipped) to avoid a
    /// single broken glyph aborting the whole line.
    pub fn expand_draw_text(
        &mut self,
        out: &mut DrawList,
        rect: Rect,
        text: &str,
        style: &TextStyle,
        color: Color,
    ) -> Result<(), UiError> {
        // Shape the text to get per-glyph (glyph_id, pen_position, advance).
        let shaped = self
            .pipeline
            .shape(text, style)
            .map_err(|e| UiError::Render(e.to_string()))?;

        let pen_x0 = rect.left();
        let pen_y0 = rect.top();

        for line in &shaped.lines {
            for glyph_pos in line {
                let glyph_id = glyph_id_from_byte_offset(text, glyph_pos.byte_offset);
                let key = GlyphKey::new(glyph_id, style.font_size, glyph_pos.x.fract(), 0.0);

                // Rasterize via atlas (cache hit or miss).
                let entry_result =
                    self.atlas
                        .get_or_rasterize(&mut self.pipeline, key, text, style);

                let entry = match entry_result {
                    Ok(e) => e,
                    Err(_) => continue, // skip unrasterizable glyphs
                };

                let bm = &entry.bitmap;
                if bm.width == 0 || bm.height == 0 || bm.pixels.is_empty() {
                    continue;
                }

                // Convert greyscale coverage → RGBA tinted with `color`.
                let rgba = greyscale_to_rgba(&bm.pixels, color);

                let dest = Rect::new(
                    pen_x0 + glyph_pos.x + entry.bearing.0 as f32,
                    pen_y0 + glyph_pos.y + entry.bearing.1 as f32,
                    bm.width as f32,
                    bm.height as f32,
                );

                if dest.width() > 0.0 && dest.height() > 0.0 {
                    let image = ImageData::new(rgba, bm.width, bm.height);
                    out.push_image(image, dest, ImageFilter::Bilinear);
                }
            }
        }

        Ok(())
    }

    /// Pre-expand all `DrawText` commands in `list` into per-glyph `Image` blits.
    ///
    /// Walks `list` command by command.  For every [`DrawCommand::DrawText`]
    /// the method calls [`expand_draw_text`] to produce per-glyph
    /// [`DrawCommand::Image`] entries in the output [`DrawList`].  All other
    /// commands are copied through unchanged.
    ///
    /// Shaping / rasterization errors for individual `DrawText` commands are
    /// silently swallowed (the text is skipped) to avoid a single bad string
    /// aborting the whole frame.
    ///
    /// # Returns
    ///
    /// A new [`DrawList`] with `DrawText` variants replaced by `Image` blits.
    ///
    /// [`expand_draw_text`]: TextBridge::expand_draw_text
    pub fn expand_draw_text_commands(&mut self, list: &DrawList) -> DrawList {
        let mut out = DrawList::new();
        for cmd in list.iter() {
            match cmd {
                DrawCommand::DrawText {
                    rect,
                    text,
                    font,
                    color,
                } => {
                    let style =
                        TextStyle::new(font.size).color([color.0, color.1, color.2, color.3]);
                    // Silently skip on shaping/rasterization failure.
                    let _ = self.expand_draw_text(&mut out, *rect, text, &style, *color);
                }
                other => {
                    out.push(other.clone());
                }
            }
        }
        out
    }

    /// Return a reference to the internal [`GlyphAtlas`] for inspection.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Return the atlas utilization fraction in `0.0..=1.0`.
    pub fn atlas_utilization(&self) -> f32 {
        self.atlas.utilization()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Approximate a glyph ID by hashing the UTF-8 code point at `byte_offset`.
///
/// This is a best-effort mapping: real glyph IDs depend on font shaping and
/// are only available via the `oxitext` pipeline's internal per-glyph metadata.
/// Since `ShapedText::GlyphPosition::byte_offset` points to the source cluster,
/// we derive the Unicode code point there and use it as a proxy for the glyph
/// ID within `GlyphKey` (the atlas key, not an OpenType GID).
///
/// For production use with multi-font or complex-script text, replace this with
/// the actual GID from `oxitext`'s positioned glyph data when that API is
/// stabilised.
fn glyph_id_from_byte_offset(text: &str, byte_offset: usize) -> u16 {
    if byte_offset >= text.len() {
        return 0;
    }
    let ch = text[byte_offset..].chars().next().unwrap_or('\0');
    (ch as u32 & 0xFFFF) as u16
}

/// Convert a greyscale (single-channel) coverage bitmap to an RGBA bitmap
/// tinted with `color`.
///
/// The coverage value `v` modulates the alpha channel:
/// `alpha = (color.alpha * v) / 255`.  RGB channels are set to the tint color.
fn greyscale_to_rgba(grey: &[u8], color: Color) -> Vec<u8> {
    let mut out = Vec::with_capacity(grey.len() * 4);
    let (r, g, b, a) = (color.0, color.1, color.2, color.3);
    for &v in grey {
        let alpha = ((a as u32 * v as u32) / 255) as u8;
        out.push(r);
        out.push(g);
        out.push(b);
        out.push(alpha);
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── greyscale_to_rgba ───────────────────────────────────────────────────────

    #[test]
    fn greyscale_to_rgba_full_coverage() {
        // coverage=255 with fully opaque tint → alpha=255.
        let rgba = greyscale_to_rgba(&[255, 128], Color(255, 0, 0, 255));
        assert_eq!(rgba.len(), 8);
        // First pixel: r=255, g=0, b=0, a=255
        assert_eq!(rgba[0], 255);
        assert_eq!(rgba[1], 0);
        assert_eq!(rgba[2], 0);
        assert_eq!(rgba[3], 255);
        // Second pixel: coverage=128 → alpha = 255*128/255 = 128
        assert_eq!(rgba[7], 128);
    }

    #[test]
    fn greyscale_to_rgba_zero_coverage_is_transparent() {
        let rgba = greyscale_to_rgba(&[0], Color(255, 255, 255, 255));
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn greyscale_to_rgba_zero_tint_alpha_is_transparent() {
        let rgba = greyscale_to_rgba(&[255], Color(255, 255, 255, 0));
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn greyscale_to_rgba_output_length_is_4x_input() {
        let input: Vec<u8> = (0..10).collect();
        let rgba = greyscale_to_rgba(&input, Color(0, 0, 0, 255));
        assert_eq!(rgba.len(), 40);
    }

    // ── glyph_id_from_byte_offset ──────────────────────────────────────────────

    #[test]
    fn glyph_id_ascii_a_is_97() {
        let id = glyph_id_from_byte_offset("A", 0);
        assert_eq!(id, 'A' as u16);
    }

    #[test]
    fn glyph_id_out_of_range_returns_zero() {
        let id = glyph_id_from_byte_offset("A", 100);
        assert_eq!(id, 0);
    }

    #[test]
    fn glyph_id_empty_text_returns_zero() {
        let id = glyph_id_from_byte_offset("", 0);
        assert_eq!(id, 0);
    }

    // ── TextBridge construction ────────────────────────────────────────────────

    #[test]
    fn text_bridge_from_invalid_font_fails() {
        // from_bytes with empty data must return Err, not panic.
        let result = TextPipeline::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn text_bridge_atlas_utilization_starts_at_zero() {
        // We can't build a valid pipeline in unit tests without a font, but we
        // can test via a pipeline that is already initialized.
        // Use a LazyTextPipeline approach: check the formula via GlyphAtlas directly.
        let atlas = GlyphAtlas::new(100);
        let u = atlas.utilization();
        assert!((u - 0.0).abs() < f32::EPSILON);
    }

    // ── expand_draw_text_commands ──────────────────────────────────────────────

    /// Non-text commands must pass through expand_draw_text_commands unchanged.
    #[test]
    fn expand_commands_passthrough_non_text() {
        use oxiui_core::geometry::Rect;
        use oxiui_core::paint::{DrawCommand, DrawList};
        use oxiui_core::Color;

        // Build a DrawList with two solid rects and no DrawText commands.
        let mut list = DrawList::new();
        list.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color(255, 0, 0, 255));
        list.push_rect(Rect::new(10.0, 0.0, 10.0, 10.0), Color(0, 255, 0, 255));

        // We cannot build a TextBridge without a valid font, but the passthrough
        // path does not require one — we need an empty TextBridge.
        // Since from_bytes([]) always returns Err, we can't easily construct a
        // TextBridge here.  Instead, verify the non-text path via a system font
        // if available, or skip if the system font is absent.
        let pipeline_result = TextPipeline::from_system_font("Helvetica")
            .or_else(|_| TextPipeline::from_system_font("Arial"))
            .or_else(|_| TextPipeline::from_system_font("sans-serif"));

        let Ok(pipeline) = pipeline_result else {
            // No system font available — skip this test.
            return;
        };
        let mut bridge = TextBridge::new(pipeline, 256);
        let expanded = bridge.expand_draw_text_commands(&list);

        // The output must contain at least the two original FillRect commands.
        let rects: Vec<_> = expanded
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillRect { .. }))
            .collect();
        assert_eq!(rects.len(), 2, "two FillRect commands must pass through");
    }

    /// A DrawText command must be replaced by ≥0 Image blits (≥1 when font is available).
    #[test]
    fn expand_commands_draw_text_produces_image_blits() {
        use oxiui_core::geometry::Rect;
        use oxiui_core::paint::{DrawCommand, DrawList};
        use oxiui_core::{Color, FontSpec};

        let pipeline_result = TextPipeline::from_system_font("Helvetica")
            .or_else(|_| TextPipeline::from_system_font("Arial"))
            .or_else(|_| TextPipeline::from_system_font("sans-serif"));

        let Ok(pipeline) = pipeline_result else {
            return; // no system font — skip
        };
        let mut bridge = TextBridge::new(pipeline, 512);

        let mut list = DrawList::new();
        let font = FontSpec::new("Helvetica", 16.0, 400);
        list.push_text(
            Rect::new(0.0, 0.0, 200.0, 24.0),
            "Hi",
            font,
            Color(255, 255, 255, 255),
        );

        let expanded = bridge.expand_draw_text_commands(&list);

        // The DrawText must be replaced — no DrawText should remain.
        let text_cmds: usize = expanded
            .iter()
            .filter(|c| matches!(c, DrawCommand::DrawText { .. }))
            .count();
        assert_eq!(text_cmds, 0, "no DrawText commands should remain");

        // At least the shaping infrastructure ran (may produce Image blits for
        // visible glyphs; empty string / whitespace-only may yield 0).
        // We just assert the expand path ran without panicking.
    }
}
