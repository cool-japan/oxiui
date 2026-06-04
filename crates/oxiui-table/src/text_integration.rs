//! `oxiui-text` integration for `oxiui-table`.
//!
//! When the `text-table` feature is enabled, cells can carry styled rich-text
//! spans rendered through `oxiui_text::TextPipeline`.  This keeps the core
//! `Cell` enum free of the `oxiui-text` dependency while enabling CJK/emoji
//! shaping and per-span formatting (bold, italic, color, letter-spacing) in the
//! renderer layer.
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "text-table")]
//! # {
//! use oxiui_table::text_integration::{RichCell, StyledSpan};
//! use oxiui_text::TextStyle;
//!
//! let mut cell = RichCell::plain("Hello, 世界!");
//! cell.push_span(StyledSpan {
//!     text: " (emoji 🎉)".to_owned(),
//!     style: TextStyle::new(16.0).color([255, 200, 0, 255]),
//! });
//! let plain = cell.plain_text();
//! assert_eq!(plain, "Hello, 世界! (emoji 🎉)");
//! # }
//! ```
//!
//! # Feature flag
//!
//! All types in this module are unconditionally compiled but only meaningful
//! when the `text-table` feature is enabled (which gates the `oxiui-text`
//! dependency).  Import paths should use `#[cfg(feature = "text-table")]` at
//! the call site when conditional.

#[cfg(feature = "text-table")]
use oxiui_text::{ShapedText, TextPipeline, TextStyle};

use crate::Cell;

// ── StyledSpan ────────────────────────────────────────────────────────────────

/// A single styled text fragment inside a [`RichCell`].
///
/// Each span carries its own [`oxiui_text::TextStyle`] so that a single table
/// cell can mix normal text with bold/italic/coloured segments and CJK or
/// emoji runs.
#[cfg(feature = "text-table")]
#[derive(Clone, Debug)]
pub struct StyledSpan {
    /// The text content (UTF-8, may include CJK code-points and emoji).
    pub text: String,
    /// Rendering style for this span.
    pub style: TextStyle,
}

/// A fallback span type used when the `text-table` feature is disabled.
///
/// This allows the `RichCell` struct to exist unconditionally while only
/// needing `oxiui_text::TextStyle` when the feature flag is on.
#[cfg(not(feature = "text-table"))]
#[derive(Clone, Debug)]
pub struct StyledSpan {
    /// The text content.
    pub text: String,
}

// ── RichCell ─────────────────────────────────────────────────────────────────

/// A table cell whose content is a sequence of [`StyledSpan`]s.
///
/// `RichCell` extends the plain [`Cell::Text`] model with per-span styling,
/// enabling mixed bold/italic/coloured text and proper CJK/emoji shaping
/// through the `TextPipeline`.
///
/// # Relationship to `Cell`
///
/// `RichCell` is intentionally separate from [`crate::Cell`] so that data
/// sources that do not need rich text are unaffected by the `oxiui-text`
/// dependency.  Renderers that understand `RichCell` can convert it to a
/// `Cell::Text` for generic rendering via [`RichCell::to_plain_cell`].
#[derive(Clone, Debug, Default)]
pub struct RichCell {
    spans: Vec<StyledSpan>,
}

impl RichCell {
    /// Create an empty `RichCell`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `RichCell` with a single plain span using the default style.
    #[cfg(feature = "text-table")]
    pub fn plain(text: impl Into<String>) -> Self {
        let mut cell = Self::new();
        cell.spans.push(StyledSpan {
            text: text.into(),
            style: TextStyle::default(),
        });
        cell
    }

    /// Create a `RichCell` with a single plain span (no-feature version).
    #[cfg(not(feature = "text-table"))]
    pub fn plain(text: impl Into<String>) -> Self {
        let mut cell = Self::new();
        cell.spans.push(StyledSpan { text: text.into() });
        cell
    }

    /// Append a span to this cell.
    pub fn push_span(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }

    /// Borrow the span list.
    pub fn spans(&self) -> &[StyledSpan] {
        &self.spans
    }

    /// Return the concatenated plain text of all spans (no style information).
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Convert to a plain [`Cell::Text`] for generic (non-rich) rendering.
    pub fn to_plain_cell(&self) -> Cell {
        Cell::Text(self.plain_text())
    }

    /// Shape and measure all spans using `pipeline`, returning per-span shaped
    /// results.
    ///
    /// Each element of the returned `Vec` corresponds to the span at the same
    /// index in [`RichCell::spans`].  The pipeline retains per-font shape
    /// caches internally, so repeated calls are faster than the first.
    ///
    /// # Errors
    ///
    /// Returns a `String` error message if any span fails to shape.  Spans
    /// before the failing one are still returned (partial success).
    #[cfg(feature = "text-table")]
    pub fn shape_spans(&self, pipeline: &mut TextPipeline) -> Result<Vec<ShapedText>, String> {
        let mut results = Vec::with_capacity(self.spans.len());
        for span in &self.spans {
            let shaped = pipeline
                .shape(&span.text, &span.style)
                .map_err(|e| e.to_string())?;
            results.push(shaped);
        }
        Ok(results)
    }

    /// Measure the combined bounding box of all spans using `pipeline`.
    ///
    /// Returns `(total_width, max_height)` in logical pixels.  Individual span
    /// widths are summed horizontally (single-line model); height is the
    /// maximum across all spans.
    ///
    /// # Errors
    ///
    /// Propagates pipeline shaping errors.
    #[cfg(feature = "text-table")]
    pub fn measure(&self, pipeline: &mut TextPipeline) -> Result<(f32, f32), String> {
        let mut total_w = 0.0_f32;
        let mut max_h = 0.0_f32;
        for span in &self.spans {
            let (w, h) = pipeline
                .measure(&span.text, &span.style)
                .map_err(|e| e.to_string())?;
            total_w += w;
            max_h = max_h.max(h);
        }
        Ok((total_w, max_h))
    }
}

// ── Cell extensions ───────────────────────────────────────────────────────────

/// Extension trait for converting `Cell` values to/from `RichCell`.
///
/// Automatically implemented for [`Cell`].
pub trait CellRichExt {
    /// Wrap this cell's display string into a single-span [`RichCell`].
    fn to_rich_cell(&self) -> RichCell;
}

impl CellRichExt for Cell {
    fn to_rich_cell(&self) -> RichCell {
        RichCell::plain(self.to_string())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_cell_default_empty() {
        let cell = RichCell::new();
        assert!(cell.spans().is_empty());
        assert_eq!(cell.plain_text(), "");
    }

    #[test]
    fn rich_cell_plain_single_span() {
        let cell = RichCell::plain("hello");
        assert_eq!(cell.spans().len(), 1);
        assert_eq!(cell.plain_text(), "hello");
    }

    #[test]
    fn rich_cell_multi_span_plain_text() {
        let mut cell = RichCell::plain("Hello, ");
        cell.push_span(RichCell::plain("世界!").spans()[0].clone());
        assert_eq!(cell.plain_text(), "Hello, 世界!");
    }

    #[test]
    fn rich_cell_to_plain_cell_is_text() {
        let cell = RichCell::plain("data");
        let plain = cell.to_plain_cell();
        assert!(matches!(plain, Cell::Text(_)));
        assert_eq!(plain.to_string(), "data");
    }

    #[test]
    fn cell_to_rich_cell_int() {
        use super::CellRichExt;
        let cell = Cell::Int(42);
        let rich = cell.to_rich_cell();
        assert_eq!(rich.plain_text(), "42");
    }

    #[test]
    fn cell_to_rich_cell_float() {
        use super::CellRichExt;
        // 3.14 is intentional test data (not PI); suppress approx_constant lint.
        #[allow(clippy::approx_constant)]
        let cell = Cell::Float(3.14_f64);
        let rich = cell.to_rich_cell();
        assert_eq!(rich.plain_text(), "3.14");
    }

    #[test]
    fn cell_to_rich_cell_bool() {
        use super::CellRichExt;
        let cell = Cell::Bool(true);
        let rich = cell.to_rich_cell();
        assert_eq!(rich.plain_text(), "true");
    }

    #[test]
    fn cell_to_rich_cell_empty() {
        use super::CellRichExt;
        let cell = Cell::Empty;
        let rich = cell.to_rich_cell();
        assert_eq!(rich.plain_text(), "");
    }

    #[test]
    fn rich_cell_cjk_codepoints_preserved() {
        // The plain_text concatenation must preserve multi-byte UTF-8 sequences.
        let cell = RichCell::plain("日本語テスト");
        assert_eq!(cell.plain_text(), "日本語テスト");
    }

    #[test]
    fn rich_cell_emoji_codepoints_preserved() {
        let cell = RichCell::plain("🎉🦀🚀");
        assert_eq!(cell.plain_text(), "🎉🦀🚀");
    }

    #[test]
    fn rich_cell_mixed_cjk_emoji_ascii() {
        let mut cell = RichCell::plain("Hello ");
        cell.push_span(RichCell::plain("世界 🌏").spans()[0].clone());
        assert_eq!(cell.plain_text(), "Hello 世界 🌏");
    }

    #[cfg(feature = "text-table")]
    #[test]
    fn styled_span_has_style_field() {
        let span = StyledSpan {
            text: "test".to_owned(),
            style: TextStyle::new(14.0),
        };
        assert!((span.style.font_size - 14.0).abs() < f32::EPSILON);
    }
}
