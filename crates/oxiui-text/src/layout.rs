//! Text layout with alignment and hit-testing.
//!
//! [`TextLayout`] wraps a [`ShapedText`] together with an alignment mode and
//! maximum bounds, providing aligned glyph positions and click-to-caret
//! hit-testing.

use crate::{GlyphPosition, ShapedText, TextPipeline, TextStyle};

// ── TextAlign ─────────────────────────────────────────────────────────────────

/// Horizontal alignment of laid-out text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Align to the left edge.
    #[default]
    Left,
    /// Centre each line within the bounds.
    Center,
    /// Align to the right edge.
    Right,
    /// Justify all complete lines (distribute inter-word spacing).
    Justify,
}

// ── TextLayout ────────────────────────────────────────────────────────────────

/// A shaped text block with alignment and bounds information.
pub struct TextLayout {
    /// The raw shaped text (glyph positions, line metrics).
    pub shaped: ShapedText,
    /// Requested alignment.
    pub align: TextAlign,
    /// `(max_width, max_height)` — the layout box dimensions.
    pub bounds: (f32, f32),
}

impl TextLayout {
    /// Shape `text` and apply `align` within `max_width`.
    ///
    /// # Errors
    /// Propagates shaping errors from the pipeline.
    pub fn new(
        pipeline: &mut TextPipeline,
        text: &str,
        style: &TextStyle,
        max_width: f32,
        align: TextAlign,
    ) -> Result<Self, crate::TextError> {
        let mut style_with_width = style.clone();
        style_with_width.max_width = max_width;
        let shaped = pipeline.shape(text, &style_with_width)?;
        let total_height = shaped.total_height;
        Ok(Self {
            shaped,
            align,
            bounds: (max_width, total_height),
        })
    }

    /// Apply alignment offsets to glyph positions within `bounds`.
    ///
    /// Returns per-line glyph positions adjusted for the requested alignment.
    pub fn align_glyphs(&self) -> Vec<Vec<GlyphPosition>> {
        let max_w = self.bounds.0;
        self.shaped
            .lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    return line.clone();
                }

                let line_w = line.iter().map(|g| g.x + g.width).fold(0.0_f32, f32::max);

                let offset_x = match self.align {
                    TextAlign::Left => 0.0,
                    TextAlign::Right => (max_w - line_w).max(0.0),
                    TextAlign::Center => ((max_w - line_w) / 2.0).max(0.0),
                    TextAlign::Justify => 0.0, // justify gaps handled below
                };

                if matches!(self.align, TextAlign::Justify) {
                    // Justify: distribute whitespace evenly between glyphs.
                    let gap = (max_w - line_w) / (line.len().saturating_sub(1).max(1)) as f32;
                    line.iter()
                        .enumerate()
                        .map(|(i, g)| GlyphPosition {
                            x: g.x + gap * i as f32,
                            ..g.clone()
                        })
                        .collect()
                } else {
                    line.iter()
                        .map(|g| GlyphPosition {
                            x: g.x + offset_x,
                            ..g.clone()
                        })
                        .collect()
                }
            })
            .collect()
    }

    /// Fast O(log n) hit-test over a single horizontal sweep using binary
    /// search over sorted glyph x-positions.
    ///
    /// Returns the **glyph index** (into the concatenated flat list of all
    /// glyphs across all lines) of the entry whose left edge is closest to `x`.
    /// This is O(log n) in the total number of glyphs, vs. the O(n) linear
    /// scan in [`Self::hit_test`].
    ///
    /// Unlike [`Self::hit_test`] this method ignores the y-coordinate and
    /// operates on the full concatenated glyph stream — callers that need
    /// per-line hit-testing should pre-filter by line before calling.
    pub fn hit_test_fast(&self, x: f32) -> usize {
        // Collect the left-edge x-position of every glyph in layout order.
        let positions: Vec<f32> = self
            .shaped
            .lines
            .iter()
            .flat_map(|line| line.iter().map(|g| g.x))
            .collect();

        if positions.is_empty() {
            return 0;
        }

        // `partition_point` returns the first index where `pos >= x`, i.e. the
        // insertion point.  The closest glyph is either at that index or one
        // before it.
        let insert = positions.partition_point(|&pos| pos < x);

        if insert == 0 {
            return 0;
        }
        if insert >= positions.len() {
            return positions.len() - 1;
        }

        // Choose whichever neighbour is closer to `x`.
        let prev = insert - 1;
        if (x - positions[prev]).abs() <= (x - positions[insert]).abs() {
            prev
        } else {
            insert
        }
    }

    /// Return the byte offset of the glyph closest to `(x, y)`.
    ///
    /// Useful for click-to-caret positioning.
    pub fn hit_test(&self, x: f32, y: f32) -> usize {
        if self.shaped.lines.is_empty() {
            return 0;
        }

        // Find the closest line by y coordinate.
        let line = {
            let mut best_line: &Vec<GlyphPosition> = &self.shaped.lines[0];
            let mut best_dist = f32::MAX;
            for line in &self.shaped.lines {
                if line.is_empty() {
                    continue;
                }
                let top = line[0].y;
                let bottom = top + line[0].height;
                let mid = (top + bottom) * 0.5;
                let dist = (y - mid).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_line = line;
                }
            }
            best_line
        };

        if line.is_empty() {
            return 0;
        }

        // Find the closest glyph by x coordinate within that line.
        let mut best_offset = line[0].byte_offset;
        let mut best_dist = f32::MAX;
        for g in line {
            let mid = g.x + g.width * 0.5;
            let dist = (x - mid).abs();
            if dist < best_dist {
                best_dist = dist;
                best_offset = g.byte_offset;
            }
        }
        best_offset
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GlyphPosition;

    fn fake_shaped(lines: Vec<Vec<GlyphPosition>>) -> ShapedText {
        let total_width = lines
            .iter()
            .flat_map(|l| l.iter())
            .map(|g| g.x + g.width)
            .fold(0.0_f32, f32::max);
        let total_height = lines
            .iter()
            .flat_map(|l| l.iter())
            .map(|g| g.y + g.height)
            .fold(0.0_f32, f32::max);
        ShapedText {
            lines,
            total_width,
            total_height,
        }
    }

    fn single_line_layout(align: TextAlign, max_w: f32) -> TextLayout {
        let line = vec![
            GlyphPosition {
                byte_offset: 0,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 16.0,
            },
            GlyphPosition {
                byte_offset: 1,
                x: 10.0,
                y: 0.0,
                width: 10.0,
                height: 16.0,
            },
        ];
        let shaped = fake_shaped(vec![line]);
        TextLayout {
            shaped,
            align,
            bounds: (max_w, 16.0),
        }
    }

    #[test]
    fn layout_left_align_starts_at_zero() {
        let layout = single_line_layout(TextAlign::Left, 200.0);
        let aligned = layout.align_glyphs();
        let first_x = aligned[0][0].x;
        assert!(
            (first_x - 0.0).abs() < f32::EPSILON,
            "left-aligned glyph should start at x=0"
        );
    }

    #[test]
    fn layout_right_align_ends_at_max_width() {
        let layout = single_line_layout(TextAlign::Right, 200.0);
        let aligned = layout.align_glyphs();
        let last = aligned[0].last().unwrap();
        let end_x = last.x + last.width;
        assert!(
            (end_x - 200.0).abs() < f32::EPSILON,
            "right-aligned line should end at max_width"
        );
    }

    #[test]
    fn layout_center_align_midpoint() {
        let layout = single_line_layout(TextAlign::Center, 100.0);
        let aligned = layout.align_glyphs();
        // Line width = 20, max_width = 100 → offset = 40
        let first_x = aligned[0][0].x;
        assert!(
            (first_x - 40.0).abs() < f32::EPSILON,
            "center first glyph x should be 40"
        );
    }

    #[test]
    fn layout_hit_test_basic() {
        let layout = single_line_layout(TextAlign::Left, 100.0);
        // Click at x=5, y=8 should hit glyph at byte_offset 0
        let offset = layout.hit_test(5.0, 8.0);
        assert_eq!(offset, 0);
        // Click at x=15 should hit byte_offset 1
        let offset2 = layout.hit_test(15.0, 8.0);
        assert_eq!(offset2, 1);
    }

    // ── hit_test_fast ─────────────────────────────────────────────────────

    #[test]
    fn hit_test_fast_returns_index_zero_for_leftmost() {
        let layout = single_line_layout(TextAlign::Left, 100.0);
        // x=0 → closest to glyph at index 0 (x=0.0)
        let idx = layout.hit_test_fast(0.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn hit_test_fast_returns_last_index_for_far_right() {
        let layout = single_line_layout(TextAlign::Left, 100.0);
        // x=100 → closest to glyph at index 1 (x=10.0), which is the last
        let idx = layout.hit_test_fast(100.0);
        assert_eq!(idx, 1);
    }

    #[test]
    fn hit_test_fast_midpoint_tie_breaks_to_left() {
        // Two glyphs at x=0 and x=10: midpoint = 5. At exactly 5 the
        // previous glyph wins (<=).
        let layout = single_line_layout(TextAlign::Left, 100.0);
        let idx = layout.hit_test_fast(5.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn hit_test_fast_empty_layout_returns_zero() {
        let shaped = ShapedText {
            lines: Vec::new(),
            total_width: 0.0,
            total_height: 0.0,
        };
        let layout = TextLayout {
            shaped,
            align: TextAlign::Left,
            bounds: (100.0, 0.0),
        };
        assert_eq!(layout.hit_test_fast(50.0), 0);
    }
}
