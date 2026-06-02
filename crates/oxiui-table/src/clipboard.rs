//! Clipboard integration for the table widget.
//!
//! Rather than depending on any OS-clipboard crate (which would introduce
//! C/C++ transitive dependencies on some platforms), the table exposes a
//! [`ClipboardSink`] trait.  Callers implement it using whatever clipboard
//! mechanism is appropriate for their application (arboard, winapi,
//! wasm-clipboard, etc.).  The table itself only builds a TSV string and
//! passes it to the sink.
//!
//! Two built-in implementations are provided for testing and headless use:
//! [`NullClipboard`] (discards copies) and [`CaptureClipboard`] (records them).

/// Trait for receiving text that the table wants to copy to the clipboard.
///
/// Implement this on a type that has access to the platform clipboard and
/// pass a mutable reference to the table's copy-handler.  The table never
/// calls any OS APIs directly.
pub trait ClipboardSink: Send + Sync {
    /// Called when the table wants to place `text` on the clipboard.
    ///
    /// The caller is responsible for forwarding `text` to the actual OS or
    /// application clipboard.
    fn copy_text(&mut self, text: String);
}

/// A clipboard sink that silently discards all copies.
///
/// Useful for headless / server-side rendering where the clipboard is not
/// available.
pub struct NullClipboard;

impl ClipboardSink for NullClipboard {
    fn copy_text(&mut self, _text: String) {}
}

/// A clipboard sink that records every copied string for later inspection.
///
/// Primarily intended for unit testing: after the operation under test, inspect
/// [`CaptureClipboard::captured`] to verify what would have been placed on the
/// clipboard.
pub struct CaptureClipboard {
    /// All text strings that have been passed to [`ClipboardSink::copy_text`]
    /// in chronological order.
    pub captured: Vec<String>,
}

impl CaptureClipboard {
    /// Create a new, empty [`CaptureClipboard`].
    pub fn new() -> Self {
        Self {
            captured: Vec::new(),
        }
    }
}

impl Default for CaptureClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardSink for CaptureClipboard {
    fn copy_text(&mut self, text: String) {
        self.captured.push(text);
    }
}

/// Convert a rectangular selection of cell strings to a TSV (tab-separated
/// values) string.
///
/// Each inner `Vec<String>` is one row.  Columns within a row are joined with
/// `\t`; rows are joined with `\n`.  An empty input produces an empty string.
///
/// # Example
///
/// ```rust
/// use oxiui_table::selection_to_tsv;
/// let cells = vec![
///     vec!["a".to_string(), "b".to_string()],
///     vec!["c".to_string(), "d".to_string()],
/// ];
/// assert_eq!(selection_to_tsv(&cells), "a\tb\nc\td");
/// ```
pub fn selection_to_tsv(cells: &[Vec<String>]) -> String {
    cells
        .iter()
        .map(|row| row.join("\t"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_export_tab_separated() {
        let cells = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
        assert_eq!(selection_to_tsv(&cells), "a\tb\tc");
    }

    #[test]
    fn tsv_export_newline_between_rows() {
        let cells = vec![
            vec!["1".to_string(), "2".to_string()],
            vec!["3".to_string(), "4".to_string()],
        ];
        assert_eq!(selection_to_tsv(&cells), "1\t2\n3\t4");
    }

    #[test]
    fn tsv_empty_input() {
        assert_eq!(selection_to_tsv(&[]), "");
    }

    #[test]
    fn capture_clipboard_records_text() {
        let mut sink = CaptureClipboard::new();
        sink.copy_text("hello".to_string());
        sink.copy_text("world".to_string());
        assert_eq!(sink.captured, vec!["hello", "world"]);
    }

    #[test]
    fn null_clipboard_does_not_panic() {
        let mut sink = NullClipboard;
        sink.copy_text("anything".to_string());
    }
}
