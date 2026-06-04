#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-text` — rich text layer bridging OxiUI to OxiText/OxiFont.
//!
//! Provides text measurement, layout, hit-testing, selection, rich text spans,
//! LRU shaping cache, font fallback, decorations, truncation, and hyperlink
//! detection — all built on pure-Rust `oxitext` + `oxifont`.

pub mod atlas;
pub mod cache;
pub mod decoration;
pub mod editor;
/// Emoji rendering support (requires `emoji` feature).
///
/// Provides [`emoji::EmojiSegmenter`], [`emoji::is_emoji_codepoint`], and
/// [`emoji::EmojiRenderer`] for mixed-text emoji rendering.
#[cfg(feature = "emoji")]
pub mod emoji;
pub mod fallback;
pub mod highlight;
pub mod hyperlink;
pub mod ime;
pub mod input;
pub mod label;
pub mod layout;
pub mod rich;
pub mod selection;
pub mod truncation;

pub use atlas::{GlyphAtlas, GlyphEntry, GlyphKey};
pub use editor::{TextArea, WrapMode};
pub use highlight::{Highlighter, KeywordHighlighter};
pub use ime::Preedit;
pub use input::TextInput;
pub use label::Label;

use oxiui_core::UiError;

// ── Re-exports from upstream ─────────────────────────────────────────────────

pub use oxitext::{ParagraphMetrics, PositionedGlyph, RenderResult};

// ── Error type ───────────────────────────────────────────────────────────────

/// All errors that can originate from the `oxiui-text` layer.
#[derive(Debug)]
pub enum TextError {
    /// An error from the underlying OxiText pipeline.
    Pipeline(oxitext::OxiTextError),
    /// A miscellaneous text error.
    Other(String),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Pipeline(e) => write!(f, "text pipeline error: {e}"),
            TextError::Other(s) => write!(f, "text error: {s}"),
        }
    }
}

impl std::error::Error for TextError {}

impl From<oxitext::OxiTextError> for TextError {
    fn from(e: oxitext::OxiTextError) -> Self {
        TextError::Pipeline(e)
    }
}

impl From<TextError> for UiError {
    fn from(e: TextError) -> Self {
        UiError::Render(e.to_string())
    }
}

// ── TextStyle builder ─────────────────────────────────────────────────────────

/// Builder-style text style for OxiUI widgets.
///
/// Wraps the upstream [`oxitext::TextStyle`] with additional per-glyph
/// attributes (bold, italic, color, letter spacing) that OxiUI layers on
/// top.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Font family name, if any.
    pub font_family: Option<String>,
    /// Font size in pixels-per-em.
    pub font_size: f32,
    /// Bold weight.
    pub bold: bool,
    /// Italic slant.
    pub italic: bool,
    /// Foreground RGBA color.
    pub color: [u8; 4],
    /// Additional horizontal spacing between glyphs, in pixels.
    pub letter_spacing: f32,
    /// Line height multiplier (1.0 = natural).
    pub line_height: f32,
    /// Maximum line width for wrapping (0 = no wrap).
    pub max_width: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(16.0)
    }
}

impl TextStyle {
    /// Create a new style with the given font size and sensible defaults.
    pub fn new(size: f32) -> Self {
        Self {
            font_family: None,
            font_size: size,
            bold: false,
            italic: false,
            color: [0, 0, 0, 255],
            letter_spacing: 0.0,
            line_height: 1.0,
            max_width: 0.0,
        }
    }

    /// Set the font family name.
    pub fn family(mut self, name: impl Into<String>) -> Self {
        self.font_family = Some(name.into());
        self
    }

    /// Enable bold weight.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Enable italic slant.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Set the foreground RGBA color.
    pub fn color(mut self, rgba: [u8; 4]) -> Self {
        self.color = rgba;
        self
    }

    /// Set the additional letter spacing in pixels.
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Set the line height multiplier.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    /// Set the maximum line width for wrapping.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    /// Convert to the upstream [`oxitext::TextStyle`].
    pub(crate) fn to_upstream(&self) -> oxitext::TextStyle {
        oxitext::TextStyle {
            font_size: self.font_size,
            max_width: self.max_width,
            flow_direction: oxitext::FlowDirection::Horizontal,
            alignment: oxitext::TextAlignment::Left,
            line_spacing: oxitext::LineSpacing::default(),
        }
    }
}

// ── GlyphPosition ─────────────────────────────────────────────────────────────

/// The position of a single glyph cluster in the laid-out text.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPosition {
    /// UTF-8 byte offset of this glyph's cluster in the source text.
    pub byte_offset: usize,
    /// Left edge in canvas pixels.
    pub x: f32,
    /// Top edge in canvas pixels (baseline − ascent).
    pub y: f32,
    /// Advance width of the glyph in pixels.
    pub width: f32,
    /// Line height in pixels (ascent + descent).
    pub height: f32,
}

// ── ShapedText ────────────────────────────────────────────────────────────────

/// The result of shaping and laying out a string of text.
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Glyph positions grouped by line.
    pub lines: Vec<Vec<GlyphPosition>>,
    /// Total width of the widest line in pixels.
    pub total_width: f32,
    /// Total height of all lines stacked in pixels.
    pub total_height: f32,
}

// ── TextPipeline ─────────────────────────────────────────────────────────────

/// End-to-end text shaping + rasterization pipeline for OxiUI.
///
/// Wraps [`oxitext::Pipeline`] and maps errors to [`TextError`] / [`UiError`].
pub struct TextPipeline {
    inner: oxitext::Pipeline,
}

impl TextPipeline {
    /// Create a new pipeline from raw font bytes (TTF or OTF).
    ///
    /// # Errors
    /// Returns [`TextError::Pipeline`] if the font bytes are invalid or
    /// unparseable.
    pub fn from_bytes(font_bytes: &[u8]) -> Result<Self, TextError> {
        Ok(Self {
            inner: oxitext::Pipeline::from_bytes(font_bytes)?,
        })
    }

    /// Create a pipeline using a named system font family.
    ///
    /// # Errors
    /// Returns [`TextError::Pipeline`] if no matching system font is found.
    pub fn from_system_font(family: &str) -> Result<Self, TextError> {
        Ok(Self {
            inner: oxitext::Pipeline::new_with_system_font(family)?,
        })
    }

    /// Configure a font fallback chain.
    ///
    /// When a glyph is `.notdef` in the primary font the pipeline walks
    /// this list to find a substitute.
    pub fn set_fallback_fonts(&mut self, fonts: Vec<Vec<u8>>) {
        self.inner.set_fallback_fonts(fonts);
    }

    /// Shape and lay out `text` under `style`, returning per-line glyph
    /// positions without rasterizing.
    ///
    /// # Errors
    /// Propagates shaping/layout errors.
    pub fn shape(&mut self, text: &str, style: &TextStyle) -> Result<ShapedText, TextError> {
        let upstream_style = style.to_upstream();
        let layout = self.inner.shape_and_layout(text, &upstream_style)?;
        let line_height = layout.metrics.total_height / layout.metrics.line_count.max(1) as f32;

        let mut shaped_lines: Vec<Vec<GlyphPosition>> = Vec::with_capacity(layout.lines.len());
        for line in &layout.lines {
            let ascent = line.metrics.ascent;
            let descent = line.metrics.descent;
            let glyph_height = ascent + descent;
            let top_y = line.metrics.baseline_y - ascent;

            let glyphs: Vec<GlyphPosition> = layout.glyphs[line.glyph_start..line.glyph_end]
                .iter()
                .map(|g| GlyphPosition {
                    byte_offset: g.cluster as usize,
                    x: g.pos.0,
                    y: top_y,
                    width: g.advance_x,
                    height: glyph_height,
                })
                .collect();
            shaped_lines.push(glyphs);
        }

        // If no lines were produced (empty text), supply one empty line.
        let _ = line_height; // consumed above

        Ok(ShapedText {
            lines: shaped_lines,
            total_width: layout.metrics.total_width,
            total_height: layout.metrics.total_height,
        })
    }

    /// Measure total bounding box without rasterizing.
    ///
    /// Returns `(width, height)` in pixels.
    ///
    /// # Errors
    /// Propagates shaping/layout errors.
    pub fn measure(&mut self, text: &str, style: &TextStyle) -> Result<(f32, f32), TextError> {
        let upstream_style = style.to_upstream();
        let metrics = self.inner.measure(text, &upstream_style)?;
        Ok((metrics.total_width, metrics.total_height))
    }

    /// Return per-glyph positions for hit-testing.
    ///
    /// # Errors
    /// Propagates shaping/layout errors.
    pub fn glyph_positions(
        &mut self,
        text: &str,
        style: &TextStyle,
    ) -> Result<Vec<GlyphPosition>, TextError> {
        let shaped = self.shape(text, style)?;
        Ok(shaped.lines.into_iter().flatten().collect())
    }

    /// Shape and rasterize `text` with the given style.
    ///
    /// Returns a [`RenderResult`] containing per-glyph bitmaps.
    ///
    /// # Errors
    /// Propagates pipeline errors as [`UiError::Render`].
    pub fn render(&mut self, text: &str, style: &TextStyle) -> Result<RenderResult, UiError> {
        let upstream_style = style.to_upstream();
        self.inner
            .render(text, &upstream_style)
            .map_err(|e| UiError::Render(e.to_string()))
    }
}

// ── LazyTextPipeline ─────────────────────────────────────────────────────────

/// A [`TextPipeline`] that defers parsing its font bytes until the first use.
///
/// Fonts can be large; this wrapper avoids upfront parsing cost by storing the
/// raw bytes and initialising the inner [`TextPipeline`] on the first call to
/// [`LazyTextPipeline::get`].
pub struct LazyTextPipeline {
    /// Raw TTF/OTF bytes.
    font_bytes: Vec<u8>,
    /// The lazily-initialised pipeline.
    inner: std::cell::OnceCell<TextPipeline>,
}

impl LazyTextPipeline {
    /// Create a new `LazyTextPipeline` that will parse `font_bytes` on demand.
    pub fn new(font_bytes: Vec<u8>) -> Self {
        Self {
            font_bytes,
            inner: std::cell::OnceCell::new(),
        }
    }

    /// Return a reference to the inner [`TextPipeline`], initialising it on
    /// the first call.
    ///
    /// # Errors
    /// Returns [`TextError::Pipeline`] if the font bytes are invalid.
    pub fn get(&self) -> Result<&TextPipeline, TextError> {
        if let Some(p) = self.inner.get() {
            return Ok(p);
        }
        let pipeline = TextPipeline::from_bytes(&self.font_bytes)?;
        // `set` fails only on a race (impossible with `&self`-only access here);
        // ignore the error and re-read the just-stored value.
        let _ = self.inner.set(pipeline);
        self.inner
            .get()
            .ok_or_else(|| TextError::Other("lazy pipeline initialisation failed".into()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `from_bytes` with empty bytes must return `Err`, not panic.
    #[test]
    fn from_bytes_result_type() {
        let result = TextPipeline::from_bytes(&[]);
        assert!(result.is_err(), "empty bytes must yield Err");
    }

    /// `TextStyle::new` produces expected defaults.
    #[test]
    fn text_style_defaults() {
        let s = TextStyle::new(24.0);
        assert!((s.font_size - 24.0).abs() < f32::EPSILON);
        assert!(!s.bold);
        assert!(!s.italic);
        assert_eq!(s.color, [0, 0, 0, 255]);
    }

    /// Builder chain should be additive / non-destructive.
    #[test]
    fn text_style_builder_chain() {
        let s = TextStyle::new(16.0)
            .bold()
            .italic()
            .color([255, 0, 0, 255])
            .letter_spacing(2.0)
            .family("Arial");
        assert!(s.bold);
        assert!(s.italic);
        assert_eq!(s.color, [255, 0, 0, 255]);
        assert_eq!(s.font_family.as_deref(), Some("Arial"));
    }

    /// `LazyTextPipeline::get` with empty bytes must return `Err`, not panic.
    #[test]
    fn lazy_pipeline_empty_bytes_is_err() {
        let lazy = LazyTextPipeline::new(vec![]);
        assert!(lazy.get().is_err(), "empty bytes must yield Err");
    }

    /// Second call to `LazyTextPipeline::get` (after error) must also return `Err`.
    #[test]
    fn lazy_pipeline_second_call_still_err() {
        let lazy = LazyTextPipeline::new(vec![]);
        let _ = lazy.get();
        assert!(
            lazy.get().is_err(),
            "repeated call with empty bytes must remain Err"
        );
    }
}
