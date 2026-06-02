//! Rich text span model.
//!
//! [`RichText`] holds a sequence of [`Span`]s, each carrying its own
//! [`TextStyle`].  The type supports splitting spans at byte boundaries,
//! merging adjacent spans that share the same style, and applying a style
//! mutation over a byte range.

use crate::TextStyle;

// ── Span ──────────────────────────────────────────────────────────────────────

/// A styled fragment of text.
#[derive(Clone, Debug)]
pub struct Span {
    /// The text content of this span.
    pub text: String,
    /// The rendering style of this span.
    pub style: TextStyle,
}

impl PartialEq for Span {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && span_styles_equal(&self.style, &other.style)
    }
}

/// Lightweight structural equality for [`TextStyle`].  Two styles are
/// considered equal when all layout-affecting fields match.
fn span_styles_equal(a: &TextStyle, b: &TextStyle) -> bool {
    a.font_family == b.font_family
        && (a.font_size - b.font_size).abs() < f32::EPSILON
        && a.bold == b.bold
        && a.italic == b.italic
        && a.color == b.color
        && (a.letter_spacing - b.letter_spacing).abs() < f32::EPSILON
        && (a.line_height - b.line_height).abs() < f32::EPSILON
}

// ── RichText ──────────────────────────────────────────────────────────────────

/// A sequence of [`Span`]s that together form a rich text document.
#[derive(Debug, Default)]
pub struct RichText {
    spans: Vec<Span>,
}

impl RichText {
    /// Create an empty [`RichText`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a span to the end of the rich text.
    pub fn push_span(&mut self, span: Span) {
        self.spans.push(span);
    }

    /// Borrow the span list.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Return the concatenated plain text of all spans.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Split a span at `byte_offset` (relative to the whole rich text).
    ///
    /// After the call the span that straddles `byte_offset` is replaced by two
    /// spans.  Offsets that fall exactly on a span boundary are no-ops.
    pub fn split_at(&mut self, byte_offset: usize) {
        let mut cursor = 0usize;
        let mut split_idx: Option<(usize, usize)> = None; // (span_index, local_offset)

        for (i, span) in self.spans.iter().enumerate() {
            let span_end = cursor + span.text.len();
            if cursor < byte_offset && byte_offset < span_end {
                split_idx = Some((i, byte_offset - cursor));
                break;
            }
            cursor = span_end;
        }

        if let Some((idx, local)) = split_idx {
            let original = self.spans.remove(idx);
            let (left_text, right_text) = original.text.split_at(local);
            let left = Span {
                text: left_text.to_owned(),
                style: original.style.clone(),
            };
            let right = Span {
                text: right_text.to_owned(),
                style: original.style,
            };
            self.spans.insert(idx, right);
            self.spans.insert(idx, left);
        }
    }

    /// Merge adjacent spans that have identical style into a single span.
    pub fn merge_adjacent(&mut self) {
        if self.spans.len() < 2 {
            return;
        }
        let mut merged: Vec<Span> = Vec::with_capacity(self.spans.len());
        for span in self.spans.drain(..) {
            if let Some(last) = merged.last_mut() {
                if span_styles_equal(&last.style, &span.style) {
                    last.text.push_str(&span.text);
                    continue;
                }
            }
            merged.push(span);
        }
        self.spans = merged;
    }

    /// Apply a style mutation to all spans (or portions thereof) within
    /// `[start, end)` byte offsets (relative to the full rich-text string).
    ///
    /// Spans are split at the boundaries as needed, then `style_fn` is called
    /// on each span fully contained in the range.
    pub fn apply_style_range(
        &mut self,
        start: usize,
        end: usize,
        style_fn: impl Fn(&mut TextStyle),
    ) {
        // First, ensure splits at the exact boundaries.
        self.split_at(start);
        self.split_at(end);

        // Now mutate every span within [start, end).
        let mut cursor = 0usize;
        for span in &mut self.spans {
            let span_start = cursor;
            let span_end = cursor + span.text.len();
            if span_start >= start && span_end <= end {
                style_fn(&mut span.style);
            }
            cursor = span_end;
        }
    }
}

// ── Trait impls ───────────────────────────────────────────────────────────────

impl From<&str> for RichText {
    fn from(s: &str) -> Self {
        let mut rt = RichText::new();
        rt.push_span(Span {
            text: s.to_owned(),
            style: TextStyle::new(16.0),
        });
        rt
    }
}

impl std::fmt::Display for RichText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for span in &self.spans {
            f.write_str(&span.text)?;
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_text_from_str() {
        let rt = RichText::from("hello");
        assert_eq!(rt.text(), "hello");
    }

    #[test]
    fn rich_text_display() {
        let rt = RichText::from("hi");
        assert_eq!(format!("{rt}"), "hi");
    }

    #[test]
    fn rich_text_push_span() {
        let mut rt = RichText::new();
        rt.push_span(Span {
            text: "foo".to_owned(),
            style: TextStyle::new(16.0),
        });
        rt.push_span(Span {
            text: "bar".to_owned(),
            style: TextStyle::new(16.0),
        });
        assert_eq!(rt.text(), "foobar");
        assert_eq!(rt.spans().len(), 2);
    }

    #[test]
    fn rich_text_split_and_merge() {
        // "hello" split at 3 → ["hel", "lo"]; merge → ["hello"]
        let mut rt = RichText::from("hello");
        assert_eq!(rt.spans().len(), 1);
        rt.split_at(3);
        assert_eq!(rt.spans().len(), 2);
        assert_eq!(rt.text(), "hello");
        rt.merge_adjacent();
        assert_eq!(rt.spans().len(), 1);
        assert_eq!(rt.text(), "hello");
    }

    #[test]
    fn rich_text_split_at_boundary_is_noop() {
        let mut rt = RichText::from("hello");
        let before = rt.spans().len();
        rt.split_at(0); // exact start — no-op
        rt.split_at(5); // exact end — no-op
        assert_eq!(rt.spans().len(), before);
    }

    #[test]
    fn rich_text_apply_style_range() {
        let mut rt = RichText::from("hello world");
        rt.apply_style_range(0, 5, |s| s.bold = true);
        // First span should now be bold.
        assert!(rt.spans()[0].style.bold);
        // Remaining span(s) should not be bold.
        assert!(!rt.spans().iter().skip(1).any(|s| s.style.bold));
    }

    #[test]
    fn rich_text_merge_adjacent_different_styles() {
        let mut rt = RichText::new();
        rt.push_span(Span {
            text: "a".to_owned(),
            style: TextStyle::new(16.0).bold(),
        });
        rt.push_span(Span {
            text: "b".to_owned(),
            style: TextStyle::new(16.0),
        });
        rt.merge_adjacent();
        // Different styles → stays as 2 spans.
        assert_eq!(rt.spans().len(), 2);
    }
}
