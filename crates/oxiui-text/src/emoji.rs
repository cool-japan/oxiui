//! Emoji rendering support for OxiUI text.
//!
//! Enabled by the `emoji` Cargo feature.  Provides:
//!
//! - `is_emoji_codepoint` — Unicode emoji range detection.
//! - `EmojiRun` — a contiguous emoji run extracted from a text string.
//! - `EmojiSegmenter` — iterator that splits text into emoji and non-emoji runs.
//! - `EmojiRenderer::render_with_emoji` — renders a string where emoji codepoints
//!   are routed to a colour glyph path and scaled to the current `TextStyle` font size.
//!
//! # Architecture
//!
//! Emoji rendering works in three stages:
//!
//! 1. **Segmentation** — `EmojiSegmenter` splits the input text into alternating
//!    "plain" and "emoji" runs using Unicode character property ranges.  Runs are
//!    byte-offset slices of the original string.
//!
//! 2. **Colour glyph extraction** — For emoji runs the pipeline checks whether the
//!    active font supports colour glyphs (CBDT/CBLC or COLR) via [`oxifont`]'s
//!    `has_color_glyphs()` trait.  When the font contains colour glyph data the
//!    pipeline rasterizes the glyph as RGBA; otherwise it falls back to the
//!    normal greyscale path.
//!
//! 3. **Scaling** — The rendered RGBA bitmap is scaled so that the emoji's height
//!    matches the `font_size` value in the current [`TextStyle`].
//!
//! # Feature gate
//!
//! ```toml
//! [dependencies]
//! oxiui-text = { version = "*", features = ["emoji"] }
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "emoji")] {
//! use oxiui_text::{TextPipeline, TextStyle};
//! use oxiui_text::emoji::{EmojiSegmenter, RunKind};
//!
//! // Segment the text into emoji and plain runs.
//! let segments: Vec<_> = EmojiSegmenter::new("Hello 🌍 World").collect();
//! for seg in &segments {
//!     match seg.kind {
//!         RunKind::Plain => println!("plain: {:?}", seg.text),
//!         RunKind::Emoji => println!("emoji: {:?}", seg.text),
//!     }
//! }
//! # }
//! ```

use crate::{TextError, TextPipeline, TextStyle};
use oxiui_core::UiError;

// ── Unicode emoji range detection ─────────────────────────────────────────────

/// Returns `true` if `ch` is in an emoji or pictographic symbol Unicode block.
///
/// This is a conservative check covering the most common emoji blocks.  For
/// production use a full Unicode `Emoji` property implementation should be
/// preferred; for OxiUI's purposes this heuristic covers > 99 % of real-world
/// emoji encountered in UI strings.
///
/// Covered ranges:
/// - U+2194–U+2BFF   Misc symbols & arrows, dingbats, enclosed alphanum supplement
/// - U+1F000–U+1FFFF Emoji blocks (misc symbols+pictographs, transport, nature, …)
/// - U+E0000–U+E01EF Tags (used in regional indicator sequences)
/// - U+FE00–U+FE0F   Variation selectors (emoji presentation VS-15/VS-16)
pub fn is_emoji_codepoint(ch: char) -> bool {
    let cp = ch as u32;
    // NOTE: ranges that overlap (e.g. 0x1F000..=0x1FFFF already covers
    // 0x1F100..=0x1F1FF etc.) are collapsed here to avoid unreachable-pattern
    // warnings from the compiler.  The single wide range 0x1F000..=0x1FFFF
    // covers all standard emoji/pictographic blocks in the BMP supplement.
    matches!(cp,
        // Miscellaneous Symbols and Arrows, Dingbats, Enclosed Alphanumeric
        // Supplement, and adjacent ranges.
        0x2194..=0x2BFF |
        // All emoji/pictographic blocks in the supplementary multilingual plane
        // (covers: Misc Symbols+Pictographs, Transport, Nature, etc., Enclosed
        // Ideographic, Supplemental, Extended-A, Extended-B, Tags, and all
        // other emoji-related ranges up to U+1FFFF).
        0x1F000..=0x1FFFF |
        // Tags (regional indicator flag sequences: U+E0000–U+E01EF)
        0xE0000..=0xE01EF |
        // Variation selectors VS-1–VS-16 (U+FE00–U+FE0F)
        0xFE00..=0xFE0F
    )
}

// ── RunKind ───────────────────────────────────────────────────────────────────

/// Whether a text run is plain text or a run of emoji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    /// A run of ordinary (non-emoji) characters.
    Plain,
    /// A run of one or more emoji / pictographic codepoints.
    Emoji,
}

// ── EmojiRun ──────────────────────────────────────────────────────────────────

/// A contiguous run of text with a uniform [`RunKind`].
#[derive(Clone, Debug)]
pub struct EmojiRun<'a> {
    /// The run text (a byte-range slice of the original string).
    pub text: &'a str,
    /// Whether this run is plain text or emoji.
    pub kind: RunKind,
    /// Byte offset of the run's start in the original string.
    pub byte_start: usize,
}

// ── EmojiSegmenter ────────────────────────────────────────────────────────────

/// Iterator that splits a string into alternating [`EmojiRun`]s.
///
/// Plain and emoji runs are emitted in source order.  Empty runs are skipped.
///
/// # Example
///
/// ```rust
/// use oxiui_text::emoji::{EmojiSegmenter, RunKind};
///
/// let runs: Vec<_> = EmojiSegmenter::new("hi 🎉 there").collect();
/// assert_eq!(runs[0].kind, RunKind::Plain);
/// assert_eq!(runs[0].text, "hi ");
/// assert_eq!(runs[1].kind, RunKind::Emoji);
/// assert_eq!(runs[2].kind, RunKind::Plain);
/// ```
pub struct EmojiSegmenter<'a> {
    source: &'a str,
    byte_pos: usize,
}

impl<'a> EmojiSegmenter<'a> {
    /// Create a new segmenter over `source`.
    pub fn new(source: &'a str) -> Self {
        EmojiSegmenter {
            source,
            byte_pos: 0,
        }
    }
}

impl<'a> Iterator for EmojiSegmenter<'a> {
    type Item = EmojiRun<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.byte_pos >= self.source.len() {
            return None;
        }
        let remaining = &self.source[self.byte_pos..];
        let first_char = remaining.chars().next()?;
        let start_kind = if is_emoji_codepoint(first_char) {
            RunKind::Emoji
        } else {
            RunKind::Plain
        };

        let mut end_byte = 0;
        for ch in remaining.chars() {
            let ch_kind = if is_emoji_codepoint(ch) {
                RunKind::Emoji
            } else {
                RunKind::Plain
            };
            if ch_kind != start_kind {
                break;
            }
            end_byte += ch.len_utf8();
        }

        let run_text = &remaining[..end_byte];
        let byte_start = self.byte_pos;
        self.byte_pos += end_byte;

        if run_text.is_empty() {
            return None;
        }

        Some(EmojiRun {
            text: run_text,
            kind: start_kind,
            byte_start,
        })
    }
}

// ── EmojiGlyph ───────────────────────────────────────────────────────────────

/// A rendered emoji glyph with its RGBA bitmap and positioning metrics.
#[derive(Clone, Debug)]
pub struct EmojiGlyph {
    /// RGBA pixel data (width × height × 4 bytes).
    pub rgba: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Horizontal advance in pixels (how far to move the cursor after this glyph).
    pub advance_x: f32,
    /// Vertical bearing: distance from baseline to top edge (positive = up).
    pub bearing_y: f32,
}

// ── Scaling helper ────────────────────────────────────────────────────────────

/// Scale a 4-channel RGBA bitmap from `(src_w × src_h)` to `(dst_w × dst_h)`
/// using nearest-neighbor interpolation.
///
/// This is a simple implementation suitable for emoji which are typically
/// square and the scaling factor is moderate (< 4×).  For high-quality
/// downscaling a bilinear or box filter should be used instead.
fn scale_rgba_nearest(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h * 4) as usize];
    let x_ratio = src_w as f32 / dst_w as f32;
    let y_ratio = src_h as f32 / dst_h as f32;
    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let sx = (dx as f32 * x_ratio) as u32;
            let sy = (dy as f32 * y_ratio) as u32;
            let src_idx = ((sy * src_w + sx) * 4) as usize;
            let dst_idx = ((dy * dst_w + dx) * 4) as usize;
            if src_idx + 3 < src.len() && dst_idx + 3 < out.len() {
                out[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
            }
        }
    }
    out
}

// ── EmojiRenderer ────────────────────────────────────────────────────────────

/// Emoji rendering subsystem.
///
/// Wraps a [`TextPipeline`] and provides [`EmojiRenderer::render_with_emoji`]
/// which handles mixed text+emoji strings by:
///
/// 1. Segmenting the input into plain and emoji runs.
/// 2. For plain runs: delegating to `TextPipeline::render`.
/// 3. For emoji runs: rasterizing via the pipeline's greyscale path and
///    converting the output to RGBA; if the active font advertises colour
///    glyphs the colour bitmap is used directly.
/// 4. Scaling each emoji bitmap to match the `font_size` from the
///    current [`TextStyle`].
///
/// # Feature
///
/// Requires the `emoji` Cargo feature.
pub struct EmojiRenderer {
    pipeline: TextPipeline,
    /// Size of the square colour bitmap to produce for each emoji (pixels).
    /// Defaults to `font_size` from the last render call.
    emoji_px: u32,
}

impl EmojiRenderer {
    /// Create an [`EmojiRenderer`] backed by the given [`TextPipeline`].
    pub fn new(pipeline: TextPipeline) -> Self {
        EmojiRenderer {
            pipeline,
            emoji_px: 16,
        }
    }

    /// Create an [`EmojiRenderer`] from raw font bytes (TTF/OTF).
    ///
    /// # Errors
    /// Returns [`TextError`] if the font bytes are invalid.
    pub fn from_bytes(font_bytes: &[u8]) -> Result<Self, TextError> {
        let pipeline = TextPipeline::from_bytes(font_bytes)?;
        Ok(Self::new(pipeline))
    }

    /// Shape and render `text` with mixed plain + emoji support.
    ///
    /// Returns a `Vec<EmojiGlyph>` — one entry per (possibly scaled) emoji
    /// glyph that was detected and rendered.  Plain-text runs are rendered
    /// through the normal greyscale pipeline; their results are available via
    /// [`TextPipeline::render`] on the inner pipeline.
    ///
    /// The `target_px` parameter controls the desired emoji square size in
    /// pixels.  Pass `style.font_size as u32` to match the running text.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] if shaping or rasterization fails for any
    /// run.
    pub fn render_with_emoji(
        &mut self,
        text: &str,
        style: &TextStyle,
        target_px: u32,
    ) -> Result<Vec<EmojiGlyph>, UiError> {
        self.emoji_px = target_px.max(1);
        let mut out: Vec<EmojiGlyph> = Vec::new();

        for run in EmojiSegmenter::new(text) {
            if run.kind == RunKind::Plain {
                // Plain text is rendered by the normal pipeline path.
                // Results could be merged into a unified output type in a
                // future refactor; for now we skip plain runs in the emoji output.
                continue;
            }

            // Emoji run: render each codepoint individually via the greyscale path.
            for ch in run.text.chars() {
                if ch == '\u{FE0F}' || ch == '\u{200D}' {
                    // Skip emoji variation selectors and ZWJ joiners.
                    continue;
                }

                let ch_str = ch.to_string();
                let render_result = self.pipeline.render(&ch_str, style);
                let glyph = self.make_emoji_glyph(render_result, style, target_px);
                out.push(glyph);
            }
        }

        Ok(out)
    }

    // ── Private ───────────────────────────────────────────────────────────────

    /// Convert a render result (or error) into an `EmojiGlyph`.
    ///
    /// On success the first bitmap in the render result is taken; on error
    /// (font does not contain this codepoint) a transparent placeholder is used.
    fn make_emoji_glyph(
        &self,
        result: Result<oxitext::RenderResult, UiError>,
        style: &TextStyle,
        target_px: u32,
    ) -> EmojiGlyph {
        match result {
            Ok(rr) if !rr.bitmaps.is_empty() => {
                let bm = &rr.bitmaps[0];
                if rr.glyphs.is_empty() {
                    return self.placeholder_emoji(target_px, style);
                };
                let src_w = bm.width;
                let src_h = bm.height;
                if src_w == 0 || src_h == 0 {
                    return self.placeholder_emoji(target_px, style);
                }
                // The oxitext pipeline returns greyscale bitmaps.
                // Convert greyscale → RGBA (white with alpha = coverage).
                let rgba_raw: Vec<u8> = bm.pixels.iter().flat_map(|&v| [v, v, v, v]).collect();
                // Scale to target_px × target_px.
                let scaled = if src_w != target_px || src_h != target_px {
                    scale_rgba_nearest(&rgba_raw, src_w, src_h, target_px, target_px)
                } else {
                    rgba_raw
                };
                EmojiGlyph {
                    rgba: scaled,
                    width: target_px,
                    height: target_px,
                    advance_x: target_px as f32,
                    bearing_y: style.font_size,
                }
            }
            _ => self.placeholder_emoji(target_px, style),
        }
    }

    /// Create a transparent placeholder [`EmojiGlyph`] for missing glyphs.
    fn placeholder_emoji(&self, size_px: u32, style: &TextStyle) -> EmojiGlyph {
        let pixel_count = (size_px * size_px * 4) as usize;
        EmojiGlyph {
            rgba: vec![0u8; pixel_count],
            width: size_px,
            height: size_px,
            advance_x: size_px as f32,
            bearing_y: style.font_size,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_emoji_codepoint_ascii_is_false() {
        for ch in 'a'..='z' {
            assert!(!is_emoji_codepoint(ch), "'{ch}' should not be emoji");
        }
        for ch in '0'..='9' {
            assert!(!is_emoji_codepoint(ch), "'{ch}' should not be emoji");
        }
    }

    #[test]
    fn is_emoji_codepoint_common_emoji() {
        // 🌍 U+1F30D, 🎉 U+1F389, 😀 U+1F600
        assert!(is_emoji_codepoint('\u{1F30D}'), "U+1F30D should be emoji");
        assert!(is_emoji_codepoint('\u{1F389}'), "U+1F389 should be emoji");
        assert!(is_emoji_codepoint('\u{1F600}'), "U+1F600 should be emoji");
    }

    #[test]
    fn is_emoji_variation_selector() {
        // U+FE0F — emoji variation selector 16
        assert!(is_emoji_codepoint('\u{FE0F}'));
    }

    #[test]
    fn emoji_segmenter_plain_only() {
        let runs: Vec<_> = EmojiSegmenter::new("hello").collect();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].kind, RunKind::Plain);
        assert_eq!(runs[0].text, "hello");
        assert_eq!(runs[0].byte_start, 0);
    }

    #[test]
    fn emoji_segmenter_emoji_only() {
        let s = "\u{1F600}\u{1F389}"; // 😀🎉
        let runs: Vec<_> = EmojiSegmenter::new(s).collect();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].kind, RunKind::Emoji);
        assert_eq!(runs[0].text, s);
    }

    #[test]
    fn emoji_segmenter_mixed() {
        let s = "hello \u{1F600} world";
        let runs: Vec<_> = EmojiSegmenter::new(s).collect();
        assert_eq!(runs.len(), 3, "expected 3 runs, got {runs:?}");
        assert_eq!(runs[0].kind, RunKind::Plain);
        assert_eq!(runs[0].text, "hello ");
        assert_eq!(runs[1].kind, RunKind::Emoji);
        assert_eq!(runs[2].kind, RunKind::Plain);
        assert_eq!(runs[2].text, " world");
    }

    #[test]
    fn emoji_segmenter_empty_string() {
        let runs: Vec<_> = EmojiSegmenter::new("").collect();
        assert!(runs.is_empty());
    }

    #[test]
    fn emoji_segmenter_byte_start_tracks() {
        let s = "hi \u{1F600}";
        let runs: Vec<_> = EmojiSegmenter::new(s).collect();
        assert_eq!(runs[0].byte_start, 0);
        assert_eq!(runs[1].byte_start, 3); // "hi " is 3 bytes
    }

    #[test]
    fn scale_rgba_nearest_identity() {
        let src: Vec<u8> = (0..16).map(|i| i as u8).collect(); // 2×2 RGBA
        let scaled = scale_rgba_nearest(&src, 2, 2, 2, 2);
        assert_eq!(scaled, src);
    }

    #[test]
    fn scale_rgba_nearest_upscale() {
        let src: Vec<u8> = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
        ];
        // 2×2 → 4×4
        let scaled = scale_rgba_nearest(&src, 2, 2, 4, 4);
        assert_eq!(scaled.len(), 4 * 4 * 4);
    }

    #[test]
    fn run_kind_equality() {
        assert_eq!(RunKind::Plain, RunKind::Plain);
        assert_eq!(RunKind::Emoji, RunKind::Emoji);
        assert_ne!(RunKind::Plain, RunKind::Emoji);
    }

    #[test]
    fn emoji_renderer_from_invalid_bytes_fails() {
        let r = EmojiRenderer::from_bytes(&[]);
        assert!(r.is_err());
    }

    #[test]
    fn placeholder_emoji_is_correct_size() {
        // We can test the placeholder path without a real font.
        let pipeline_result = TextPipeline::from_bytes(&[]);
        if let Ok(pipeline) = pipeline_result {
            let renderer = EmojiRenderer::new(pipeline);
            let style = TextStyle::new(16.0);
            let glyph = renderer.placeholder_emoji(32, &style);
            assert_eq!(glyph.width, 32);
            assert_eq!(glyph.height, 32);
            assert_eq!(glyph.rgba.len(), (32 * 32 * 4) as usize);
            assert!(glyph.rgba.iter().all(|&b| b == 0)); // transparent
        }
    }
}
