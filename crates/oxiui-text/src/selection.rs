//! Text selection model: anchor/focus positions, byte↔grapheme mapping,
//! highlight rect computation, and word/line boundary navigation.

use crate::GlyphPosition;

// ── Selection ─────────────────────────────────────────────────────────────────

/// A text selection defined by two byte offsets into the source string.
///
/// When `anchor == focus` the selection is a collapsed caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Byte offset where the selection started (the fixed end).
    pub anchor: usize,
    /// Byte offset of the current cursor / the moving end.
    pub focus: usize,
}

impl Selection {
    /// Create a collapsed selection (caret) at `offset`.
    pub fn new(offset: usize) -> Self {
        Self {
            anchor: offset,
            focus: offset,
        }
    }

    /// Extend the selection so that the focus moves to `focus`.
    pub fn extend_to(&mut self, focus: usize) {
        self.focus = focus;
    }

    /// Returns `true` when anchor and focus are at the same position.
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    /// Returns `(start, end)` in ascending byte-offset order.
    pub fn normalized(&self) -> (usize, usize) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    // ── Grapheme ↔ Byte conversion ────────────────────────────────────────

    /// Convert a UTF-8 byte offset to a grapheme cluster index.
    ///
    /// A "grapheme cluster" here is defined as a sequence of bytes whose
    /// first byte has the form `0b0xxx_xxxx` (ASCII) or `0b11xx_xxxx`
    /// (leading multibyte byte).  This is a simplified model; for full
    /// Unicode grapheme cluster segmentation use the `unicode-segmentation`
    /// crate (not a dependency here per the Pure Rust / no-extra-crate
    /// policy).
    pub fn byte_to_grapheme(text: &str, byte_offset: usize) -> usize {
        let capped = byte_offset.min(text.len());
        text[..capped].char_indices().count()
    }

    /// Convert a grapheme cluster index to the byte offset of its first byte.
    pub fn grapheme_to_byte(text: &str, grapheme_idx: usize) -> usize {
        text.char_indices()
            .nth(grapheme_idx)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    }

    // ── Highlight rects ───────────────────────────────────────────────────

    /// Compute highlight rectangles for the selected region.
    ///
    /// Returns `Vec<(x, y, w, h)>` — one rect per line that is (even
    /// partially) covered by the selection.
    pub fn highlight_rects(
        &self,
        glyphs: &[Vec<GlyphPosition>],
        line_height: f32,
    ) -> Vec<(f32, f32, f32, f32)> {
        if self.is_collapsed() {
            return Vec::new();
        }
        let (sel_start, sel_end) = self.normalized();
        let mut rects: Vec<(f32, f32, f32, f32)> = Vec::new();

        for line in glyphs {
            if line.is_empty() {
                continue;
            }
            // Find glyphs that overlap the selection range.
            let mut x_start: Option<f32> = None;
            let mut x_end = 0.0_f32;
            let mut line_y = 0.0_f32;

            for glyph in line {
                if glyph.byte_offset >= sel_end {
                    break;
                }
                if glyph.byte_offset >= sel_start {
                    if x_start.is_none() {
                        x_start = Some(glyph.x);
                        line_y = glyph.y;
                    }
                    x_end = glyph.x + glyph.width;
                }
            }

            if let Some(x0) = x_start {
                let w = (x_end - x0).max(1.0);
                rects.push((x0, line_y, w, line_height));
            }
        }
        rects
    }

    // ── Word navigation ───────────────────────────────────────────────────

    /// Return the byte offset just past the end of the word that starts at
    /// or after `byte_offset`.
    pub fn extend_word_forward(text: &str, byte_offset: usize) -> usize {
        if byte_offset >= text.len() {
            return text.len();
        }
        let rest = &text[byte_offset..];
        // Skip leading whitespace first.
        let leading: usize = rest
            .char_indices()
            .take_while(|(_, c)| c.is_whitespace())
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        // Then skip to end of next non-whitespace word.
        let word_end: usize = rest[leading..]
            .char_indices()
            .take_while(|(_, c)| !c.is_whitespace())
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        byte_offset + leading + word_end
    }

    /// Return the byte offset of the start of the word at or before
    /// `byte_offset`.
    pub fn extend_word_backward(text: &str, byte_offset: usize) -> usize {
        let capped = byte_offset.min(text.len());
        let before = &text[..capped];
        // Skip trailing whitespace.
        let trailing: usize = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_whitespace())
            .last()
            .map(|(i, _)| i)
            .unwrap_or(capped);
        // Then walk back to the start of the preceding non-whitespace word.
        let word_start: usize = before[..trailing]
            .char_indices()
            .rev()
            .take_while(|(_, c)| !c.is_whitespace())
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        word_start
    }

    // ── Line navigation ───────────────────────────────────────────────────

    /// Return the byte offset of the first glyph on the line that contains
    /// `byte_offset`.
    pub fn extend_line_start(glyphs: &[Vec<GlyphPosition>], byte_offset: usize) -> usize {
        for line in glyphs {
            let offsets: Vec<usize> = line.iter().map(|g| g.byte_offset).collect();
            if offsets.contains(&byte_offset)
                || (offsets.first().copied().unwrap_or(usize::MAX) <= byte_offset
                    && offsets.last().copied().unwrap_or(0) >= byte_offset)
            {
                return offsets.first().copied().unwrap_or(0);
            }
        }
        0
    }

    /// Return the byte offset past the last glyph on the line that contains
    /// `byte_offset`.
    pub fn extend_line_end(glyphs: &[Vec<GlyphPosition>], byte_offset: usize) -> usize {
        for line in glyphs {
            if line.is_empty() {
                continue;
            }
            let first = line.first().map(|g| g.byte_offset).unwrap_or(usize::MAX);
            let last = line.last().map(|g| g.byte_offset).unwrap_or(0);
            if first <= byte_offset && byte_offset <= last {
                // Offset past the last glyph on this line.
                return last + line.last().map(|g| g.width.round() as usize).unwrap_or(1);
            }
        }
        byte_offset
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_collapsed_is_caret() {
        assert!(Selection::new(5).is_collapsed());
    }

    #[test]
    fn selection_extend_not_collapsed() {
        let mut sel = Selection::new(0);
        sel.extend_to(5);
        assert!(!sel.is_collapsed());
    }

    #[test]
    fn selection_normalized_order() {
        let sel = Selection {
            anchor: 10,
            focus: 3,
        };
        assert_eq!(sel.normalized(), (3, 10));
    }

    #[test]
    fn selection_extend_forward_to_word() {
        // "hello world" — start at 0 → forward to end of "hello" = 5
        assert_eq!(Selection::extend_word_forward("hello world", 0), 5);
    }

    #[test]
    fn selection_extend_word_forward_skips_whitespace() {
        // Start at 5 (space) → skip space, skip "world" → 11
        assert_eq!(Selection::extend_word_forward("hello world", 5), 11);
    }

    #[test]
    fn selection_extend_word_backward_basic() {
        // "hello world", offset=11 → start of "world" = 6
        assert_eq!(Selection::extend_word_backward("hello world", 11), 6);
    }

    #[test]
    fn selection_grapheme_byte_roundtrip() {
        let text = "héllo";
        for (byte_off, _) in text.char_indices() {
            let g = Selection::byte_to_grapheme(text, byte_off);
            let recovered = Selection::grapheme_to_byte(text, g);
            assert_eq!(recovered, byte_off);
        }
    }

    #[test]
    fn selection_highlight_rect_count() {
        // Build a fake single-line layout.
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
            GlyphPosition {
                byte_offset: 2,
                x: 20.0,
                y: 0.0,
                width: 10.0,
                height: 16.0,
            },
        ];
        let glyphs = vec![line];
        let sel = Selection {
            anchor: 0,
            focus: 3,
        };
        let rects = sel.highlight_rects(&glyphs, 16.0);
        assert!(!rects.is_empty(), "non-empty selection must yield ≥1 rect");
    }

    #[test]
    fn selection_collapsed_no_highlight() {
        let glyphs: Vec<Vec<GlyphPosition>> = vec![vec![GlyphPosition {
            byte_offset: 0,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 16.0,
        }]];
        let sel = Selection::new(0);
        assert!(sel.highlight_rects(&glyphs, 16.0).is_empty());
    }
}
