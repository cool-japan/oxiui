//! Syntax and keyword highlighting.
//!
//! Defines the [`Highlighter`] trait and a reference [`KeywordHighlighter`]
//! implementation that requires no external parser dependency.

use std::ops::Range;

use crate::rich::{RichText, Span};
use crate::TextStyle;

// ── Highlighter trait ─────────────────────────────────────────────────────────

/// A syntax or keyword highlighter.
///
/// Implementors receive a single line of text and return a list of byte-range
/// / [`TextStyle`] pairs.  The default `apply` method builds a [`RichText`]
/// by weaving highlighted spans with plain-text spans.
pub trait Highlighter: Send + Sync {
    /// Highlight a single line of text.
    ///
    /// Returns `(byte_range, style)` pairs.  Ranges must be valid UTF-8
    /// boundaries within `line` and should not overlap.
    fn highlight_line(&self, line: &str) -> Vec<(Range<usize>, TextStyle)>;

    /// Apply highlighting to a multi-line text string, returning a [`RichText`].
    ///
    /// Lines are split on `'\n'`; a trailing newline span is appended after
    /// each line.
    fn apply(&self, text: &str) -> RichText {
        let mut rt = RichText::new();
        for line in text.split('\n') {
            let highlights = self.highlight_line(line);
            let mut pos = 0usize;
            for (range, style) in &highlights {
                if range.start > pos {
                    rt.push_span(Span {
                        text: line[pos..range.start].to_owned(),
                        style: TextStyle::default(),
                    });
                }
                rt.push_span(Span {
                    text: line[range.clone()].to_owned(),
                    style: style.clone(),
                });
                pos = range.end;
            }
            if pos < line.len() {
                rt.push_span(Span {
                    text: line[pos..].to_owned(),
                    style: TextStyle::default(),
                });
            }
            rt.push_span(Span {
                text: "\n".to_owned(),
                style: TextStyle::default(),
            });
        }
        rt
    }
}

// ── KeywordHighlighter ────────────────────────────────────────────────────────

/// A simple keyword-set based highlighter.
///
/// Scans each line for whole-word occurrences of the configured keyword list
/// and tags them with the supplied style.  No external parser dependency.
#[derive(Debug, Clone)]
pub struct KeywordHighlighter {
    keywords: Vec<String>,
    keyword_style: TextStyle,
}

impl KeywordHighlighter {
    /// Create a new `KeywordHighlighter` with the given keyword list and style.
    pub fn new(keywords: impl IntoIterator<Item = impl Into<String>>, style: TextStyle) -> Self {
        Self {
            keywords: keywords.into_iter().map(|k| k.into()).collect(),
            keyword_style: style,
        }
    }

    /// Create a `KeywordHighlighter` pre-loaded with common Rust keywords.
    pub fn with_rust_keywords(style: TextStyle) -> Self {
        Self::new(
            [
                "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "type",
                "const", "static", "if", "else", "match", "for", "while", "loop", "return",
                "break", "continue", "in", "where", "async", "await", "move", "dyn", "Box", "Vec",
                "String", "Option", "Result",
            ],
            style,
        )
    }
}

impl Highlighter for KeywordHighlighter {
    fn highlight_line(&self, line: &str) -> Vec<(Range<usize>, TextStyle)> {
        let mut result: Vec<(Range<usize>, TextStyle)> = Vec::new();

        for keyword in &self.keywords {
            let mut search_start = 0usize;
            while let Some(pos) = line[search_start..].find(keyword.as_str()) {
                let abs_start = search_start + pos;
                let abs_end = abs_start + keyword.len();

                // Word-boundary check: adjacent characters must not be alphanumeric or '_'.
                let prev_is_word = abs_start > 0
                    && line[..abs_start]
                        .chars()
                        .next_back()
                        .map(|c| c.is_alphanumeric() || c == '_')
                        .unwrap_or(false);
                let next_is_word = abs_end < line.len()
                    && line[abs_end..]
                        .chars()
                        .next()
                        .map(|c| c.is_alphanumeric() || c == '_')
                        .unwrap_or(false);

                if !prev_is_word && !next_is_word {
                    result.push((abs_start..abs_end, self.keyword_style.clone()));
                }
                search_start = abs_end;
            }
        }

        // Sort by start position; deduplicate overlapping ranges (keep first).
        result.sort_by_key(|(r, _)| r.start);
        result
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn bold_style() -> TextStyle {
        TextStyle::new(16.0).bold()
    }

    #[test]
    fn keyword_highlighter_finds_fn_keyword() {
        let h = KeywordHighlighter::new(["fn"], bold_style());
        let spans = h.highlight_line("fn main() {}");
        assert!(!spans.is_empty(), "must find 'fn'");
        assert_eq!(spans[0].0, 0..2);
    }

    #[test]
    fn keyword_highlighter_respects_word_boundary() {
        let h = KeywordHighlighter::new(["fn"], bold_style());
        // "fun" must not match "fn"
        let spans = h.highlight_line("fun and games");
        assert!(spans.is_empty(), "'fn' must not match inside 'fun'");
    }

    #[test]
    fn keyword_highlighter_apply_returns_richtext() {
        let h = KeywordHighlighter::with_rust_keywords(bold_style());
        let rt = h.apply("let x = 1;");
        let plain = rt.text();
        assert!(
            plain.contains("let"),
            "apply output must contain original text"
        );
    }

    #[test]
    fn keyword_highlighter_multiple_keywords_on_line() {
        let h = KeywordHighlighter::new(["pub", "fn"], bold_style());
        let spans = h.highlight_line("pub fn foo() {}");
        // Should find both "pub" and "fn".
        assert!(
            spans.len() >= 2,
            "both 'pub' and 'fn' should be highlighted"
        );
    }

    #[test]
    fn keyword_not_found_returns_empty() {
        let h = KeywordHighlighter::new(["struct"], bold_style());
        let spans = h.highlight_line("let x = 1;");
        assert!(spans.is_empty());
    }
}
