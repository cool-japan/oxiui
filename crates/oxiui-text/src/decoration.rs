//! Text decoration line segments (underline, overline, strikethrough).
//!
//! Produces [`DecorationSegment`]s from a slice of [`GlyphPosition`]s,
//! ready to be passed to a renderer.

use crate::GlyphPosition;

// ── DecorationStyle ───────────────────────────────────────────────────────────

/// Visual style of a decoration line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationStyle {
    /// A continuous solid line.
    Solid,
    /// A dashed line (repeating dash/gap pattern).
    Dashed,
    /// A dotted line (repeating dot/gap pattern).
    Dotted,
    /// A wavy line (sinusoidal).
    Wavy,
}

// ── TextDecoration ────────────────────────────────────────────────────────────

/// Decoration specification for a run of text.
#[derive(Debug, Clone)]
pub struct TextDecoration {
    /// Underline, drawn below the baseline.
    pub underline: Option<DecorationStyle>,
    /// Overline, drawn above the ascender.
    pub overline: Option<DecorationStyle>,
    /// Strikethrough, drawn through the x-height midpoint.
    pub strikethrough: Option<DecorationStyle>,
    /// RGBA color of all decoration lines in this spec.
    pub color: [u8; 4],
    /// Thickness of the decoration line in pixels.
    pub thickness: f32,
}

impl Default for TextDecoration {
    fn default() -> Self {
        Self {
            underline: None,
            overline: None,
            strikethrough: None,
            color: [0, 0, 0, 255],
            thickness: 1.0,
        }
    }
}

impl TextDecoration {
    /// Compute decoration line segments for a slice of glyphs on one line.
    ///
    /// - `glyphs` — the positioned glyphs on one line.
    /// - `baseline_y` — y-coordinate of the text baseline in canvas pixels.
    /// - `line_height` — ascent + descent of the line in pixels.
    ///
    /// Returns one [`DecorationSegment`] per enabled decoration type.  If
    /// `glyphs` is empty, returns an empty `Vec`.
    pub fn line_segments(
        &self,
        glyphs: &[GlyphPosition],
        baseline_y: f32,
        line_height: f32,
    ) -> Vec<DecorationSegment> {
        if glyphs.is_empty() {
            return Vec::new();
        }

        let x1 = glyphs.iter().map(|g| g.x).fold(f32::MAX, f32::min);
        let x2 = glyphs
            .iter()
            .map(|g| g.x + g.width)
            .fold(f32::MIN, f32::max);

        let mut segs: Vec<DecorationSegment> = Vec::new();

        if let Some(style) = self.underline {
            // Underline: slightly below the baseline (1–2 px gap is conventional).
            let y = baseline_y + self.thickness;
            segs.push(DecorationSegment {
                x1,
                y1: y,
                x2,
                y2: y,
                style,
                color: self.color,
                thickness: self.thickness,
            });
        }

        if let Some(style) = self.overline {
            // Overline: above the top of the glyph bounding box.
            let ascent = glyphs.iter().map(|g| g.height).fold(0.0_f32, f32::max);
            let y = baseline_y - ascent;
            segs.push(DecorationSegment {
                x1,
                y1: y,
                x2,
                y2: y,
                style,
                color: self.color,
                thickness: self.thickness,
            });
        }

        if let Some(style) = self.strikethrough {
            // Strikethrough: at the midpoint of the line height.
            let y = baseline_y - line_height * 0.25;
            segs.push(DecorationSegment {
                x1,
                y1: y,
                x2,
                y2: y,
                style,
                color: self.color,
                thickness: self.thickness,
            });
        }

        segs
    }
}

// ── DecorationSegment ─────────────────────────────────────────────────────────

/// A single decoration line segment ready to be drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct DecorationSegment {
    /// Left endpoint x.
    pub x1: f32,
    /// Left endpoint y.
    pub y1: f32,
    /// Right endpoint x.
    pub x2: f32,
    /// Right endpoint y.
    pub y2: f32,
    /// Visual style.
    pub style: DecorationStyle,
    /// RGBA color.
    pub color: [u8; 4],
    /// Line thickness in pixels.
    pub thickness: f32,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_glyphs() -> Vec<GlyphPosition> {
        vec![
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
        ]
    }

    #[test]
    fn decoration_underline_segment() {
        let dec = TextDecoration {
            underline: Some(DecorationStyle::Solid),
            ..TextDecoration::default()
        };
        let segs = dec.line_segments(&fake_glyphs(), 12.0, 16.0);
        assert_eq!(segs.len(), 1, "one segment per decoration type");
        let seg = &segs[0];
        assert_eq!(seg.style, DecorationStyle::Solid);
        assert!((seg.x1 - 0.0).abs() < f32::EPSILON);
        assert!((seg.x2 - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decoration_multiple_types() {
        let dec = TextDecoration {
            underline: Some(DecorationStyle::Solid),
            overline: Some(DecorationStyle::Dashed),
            strikethrough: Some(DecorationStyle::Dotted),
            ..TextDecoration::default()
        };
        let segs = dec.line_segments(&fake_glyphs(), 12.0, 16.0);
        assert_eq!(segs.len(), 3);
    }

    #[test]
    fn decoration_empty_glyphs_returns_empty() {
        let dec = TextDecoration {
            underline: Some(DecorationStyle::Solid),
            ..TextDecoration::default()
        };
        assert!(dec.line_segments(&[], 12.0, 16.0).is_empty());
    }

    #[test]
    fn decoration_no_decorations_no_segments() {
        let dec = TextDecoration::default();
        assert!(dec.line_segments(&fake_glyphs(), 12.0, 16.0).is_empty());
    }
}
