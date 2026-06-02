//! Static text display widget state.
//!
//! [`Label`] tracks the displayed text string and layout constraints.  Pixel-
//! accurate truncation requires a `TextPipeline` and is handled by adapters
//! calling `crate::truncation::truncate`; the `is_truncated` flag lets callers
//! communicate the result back to this state struct.

// ── Label ─────────────────────────────────────────────────────────────────────

/// State for a static text label widget.
///
/// This is a pure data structure.  Rendering and actual pixel-accurate
/// truncation are handled by the caller (adapter layer) using the functions in
/// `crate::truncation`.
#[derive(Debug, Clone)]
pub struct Label {
    text: String,
    /// Maximum number of lines to display before clipping or truncating.
    max_lines: Option<usize>,
    /// Set by the adapter when the text had to be truncated to fit.
    truncated: bool,
}

impl Label {
    /// Create a new `Label` with the given text and no line limit.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            max_lines: None,
            truncated: false,
        }
    }

    /// Limit the label to `n` visible lines.
    ///
    /// When `n` is `1`, adapters should apply single-line ellipsis truncation
    /// via `crate::truncation::truncate`.
    pub fn with_max_lines(mut self, n: usize) -> Self {
        self.max_lines = Some(n);
        self
    }

    /// The raw text stored in this label.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The optional maximum line count.
    pub fn max_lines(&self) -> Option<usize> {
        self.max_lines
    }

    /// Returns `true` when an adapter reported that truncation occurred during
    /// the last layout pass.
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Called by adapters after each layout pass to record whether truncation
    /// occurred.
    pub fn set_truncated(&mut self, truncated: bool) {
        self.truncated = truncated;
    }

    /// Returns the text that should be displayed.
    ///
    /// For single-line labels (`max_lines == Some(1)`), pixel-accurate
    /// truncation requires a `TextPipeline`; call `crate::truncation::truncate`
    /// from the adapter and pass `set_truncated(true)` when the result
    /// differs from the raw text.  This method returns the full raw text so the
    /// adapter can measure and decide.
    pub fn display_text(&self) -> &str {
        &self.text
    }
}

impl Default for Label {
    fn default() -> Self {
        Self::new("")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_new() {
        let label = Label::new("hello");
        assert_eq!(label.text(), "hello");
        assert!(label.max_lines().is_none());
    }

    #[test]
    fn label_is_truncated_false_initially() {
        let label = Label::new("hello world");
        assert!(
            !label.is_truncated(),
            "newly created label must not be truncated"
        );
    }

    #[test]
    fn label_set_truncated() {
        let mut label = Label::new("a very long text");
        assert!(!label.is_truncated());
        label.set_truncated(true);
        assert!(label.is_truncated());
    }

    #[test]
    fn label_with_max_lines() {
        let label = Label::new("text").with_max_lines(1);
        assert_eq!(label.max_lines(), Some(1));
    }

    #[test]
    fn label_display_text_matches_raw() {
        let label = Label::new("hello");
        assert_eq!(label.display_text(), "hello");
    }

    #[test]
    fn label_default() {
        let label = Label::default();
        assert_eq!(label.text(), "");
        assert!(!label.is_truncated());
    }
}
