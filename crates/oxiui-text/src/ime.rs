//! IME (Input Method Editor) composition state.
//!
//! Tracks the preedit string and cursor position within the composition, and
//! provides helpers to compute underline decoration segments and the
//! composition window rectangle.

use crate::decoration::{DecorationSegment, DecorationStyle, TextDecoration};
use crate::GlyphPosition;

// ── Preedit ───────────────────────────────────────────────────────────────────

/// IME composition (preedit) state.
///
/// Represents the intermediate text that has been entered but not yet
/// committed by the input method.
#[derive(Debug, Clone, Default)]
pub struct Preedit {
    /// The composition string currently being typed.
    pub text: String,
    /// Byte offset range of the cursor within the preedit text, if any.
    /// The first element is the start, the second is the end (exclusive).
    pub cursor_range: Option<(usize, usize)>,
}

impl Preedit {
    /// Create a new `Preedit` with the given text and optional cursor range.
    pub fn new(text: impl Into<String>, cursor_range: Option<(usize, usize)>) -> Self {
        Self {
            text: text.into(),
            cursor_range,
        }
    }

    /// Returns `true` when there is no active composition text.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Generate underline [`DecorationSegment`]s for the preedit text.
    ///
    /// `x_start` is the x-coordinate (in logical pixels) where the preedit
    /// text begins.  `y_baseline` is the y-coordinate of the text baseline.
    /// `char_width` is used as an approximate advance width per character.
    ///
    /// Returns an empty `Vec` when the preedit text is empty.
    pub fn underline_segments(
        &self,
        x_start: f32,
        y_baseline: f32,
        char_width: f32,
    ) -> Vec<DecorationSegment> {
        if self.text.is_empty() {
            return Vec::new();
        }

        let char_count = self.text.chars().count();
        let total_width = char_count as f32 * char_width;

        // Build a synthetic single-glyph slice spanning the whole preedit.
        // `line_height` is approximated as ascent (char_width * 1.2) + descent
        // (char_width * 0.2), giving a round 1.4 × char_width.
        let line_height = char_width * 1.4;
        let ascent = char_width * 1.2;
        let glyph = GlyphPosition {
            byte_offset: 0,
            x: x_start,
            y: y_baseline - ascent,
            width: total_width,
            height: line_height,
        };
        let glyph_slice = [glyph];

        let decoration = TextDecoration {
            underline: Some(DecorationStyle::Solid),
            overline: None,
            strikethrough: None,
            color: [0, 0, 0, 255],
            thickness: 1.0,
        };

        decoration.line_segments(&glyph_slice, y_baseline, line_height)
    }

    /// Compute the composition window rectangle relative to the insertion caret.
    ///
    /// Returns `(x, y, width, height)` in logical pixels.  The width is the
    /// approximate pixel width of the preedit text; the height equals
    /// `line_height`.
    pub fn composition_window_rect(
        &self,
        caret_x: f32,
        caret_y: f32,
        char_width: f32,
        line_height: f32,
    ) -> (f32, f32, f32, f32) {
        let width = (self.text.chars().count() as f32 * char_width).max(1.0);
        (caret_x, caret_y, width, line_height)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_empty_no_segments() {
        let p = Preedit::new("", None);
        assert!(p.is_empty());
        let segs = p.underline_segments(0.0, 12.0, 8.0);
        assert!(segs.is_empty(), "empty preedit must yield no segments");
    }

    #[test]
    fn preedit_underline_segments_non_empty() {
        let p = Preedit::new("こんにちは", None);
        assert!(!p.is_empty());
        let segs = p.underline_segments(0.0, 16.0, 10.0);
        assert!(
            !segs.is_empty(),
            "non-empty preedit must produce at least one underline segment"
        );
    }

    #[test]
    fn preedit_underline_segment_covers_width() {
        let p = Preedit::new("abc", None);
        let char_w = 8.0_f32;
        let segs = p.underline_segments(0.0, 16.0, char_w);
        assert!(!segs.is_empty());
        let seg = &segs[0];
        // The segment must span from x_start to x_start + 3*char_w.
        assert!(
            (seg.x2 - seg.x1 - 3.0 * char_w).abs() < f32::EPSILON,
            "underline segment width should equal char_count * char_width, got {}",
            seg.x2 - seg.x1
        );
    }

    #[test]
    fn preedit_composition_window_rect_positive_size() {
        let p = Preedit::new("hello", None);
        let (x, y, w, h) = p.composition_window_rect(10.0, 20.0, 8.0, 16.0);
        assert!((x - 10.0).abs() < f32::EPSILON);
        assert!((y - 20.0).abs() < f32::EPSILON);
        assert!(w > 0.0, "width must be positive");
        assert!((h - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn preedit_composition_window_rect_empty_has_min_width() {
        let p = Preedit::new("", None);
        let (_, _, w, _) = p.composition_window_rect(0.0, 0.0, 8.0, 16.0);
        assert!(
            w >= 1.0,
            "even empty preedit window must be at least 1px wide"
        );
    }

    #[test]
    fn preedit_cursor_range_stored() {
        let p = Preedit::new("abc", Some((1, 2)));
        assert_eq!(p.cursor_range, Some((1, 2)));
    }
}
